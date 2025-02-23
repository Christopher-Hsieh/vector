use crate::config::Resource;
use crate::event::Event;
use futures::channel::mpsc;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub use vector_core::buffers::*;

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum BufferConfig {
    Memory {
        #[serde(default = "BufferConfig::memory_max_events")]
        max_events: usize,
        #[serde(default)]
        when_full: WhenFull,
    },
    #[cfg(feature = "disk-buffer")]
    Disk {
        max_size: usize,
        #[serde(default)]
        when_full: WhenFull,
    },
}

impl Default for BufferConfig {
    fn default() -> Self {
        BufferConfig::Memory {
            max_events: BufferConfig::memory_max_events(),
            when_full: Default::default(),
        }
    }
}

impl BufferConfig {
    #[inline]
    const fn memory_max_events() -> usize {
        500
    }

    #[cfg_attr(not(feature = "disk-buffer"), allow(unused))]
    pub fn build(
        &self,
        data_dir: &Option<PathBuf>,
        sink_name: &str,
    ) -> Result<
        (
            BufferInputCloner,
            Box<dyn Stream<Item = Event> + Send>,
            Acker,
        ),
        String,
    > {
        match &self {
            BufferConfig::Memory {
                max_events,
                when_full,
            } => {
                let (tx, rx) = mpsc::channel(*max_events);
                let tx = BufferInputCloner::Memory(tx, *when_full);
                let rx = Box::new(rx);
                Ok((tx, rx, Acker::Null))
            }

            #[cfg(feature = "disk-buffer")]
            BufferConfig::Disk {
                max_size,
                when_full,
            } => {
                let data_dir = data_dir
                    .as_ref()
                    .ok_or_else(|| "Must set data_dir to use on-disk buffering.".to_string())?;
                let buffer_dir = format!("{}_buffer", sink_name);

                let (tx, rx, acker) = disk::open(&data_dir, buffer_dir.as_ref(), *max_size)
                    .map_err(|error| error.to_string())?;
                let tx = BufferInputCloner::Disk(tx, *when_full);
                Ok((tx, rx, acker))
            }
        }
    }

    /// Resources that the sink is using.
    #[cfg_attr(not(feature = "disk-buffer"), allow(unused))]
    pub fn resources(&self, sink_name: &str) -> Vec<Resource> {
        match self {
            BufferConfig::Memory { .. } => Vec::new(),
            #[cfg(feature = "disk-buffer")]
            BufferConfig::Disk { .. } => vec![Resource::DiskBuffer(sink_name.to_string())],
        }
    }
}

#[cfg(test)]
mod test {
    use crate::buffers::{BufferConfig, WhenFull};

    #[test]
    fn config_default_values() {
        fn check(source: &str, config: BufferConfig) {
            let conf: BufferConfig = toml::from_str(source).unwrap();
            assert_eq!(toml::to_string(&conf), toml::to_string(&config));
        }

        check(
            r#"
          type = "memory"
          "#,
            BufferConfig::Memory {
                max_events: 500,
                when_full: WhenFull::Block,
            },
        );

        check(
            r#"
          type = "memory"
          max_events = 100
          "#,
            BufferConfig::Memory {
                max_events: 100,
                when_full: WhenFull::Block,
            },
        );

        check(
            r#"
          type = "memory"
          when_full = "drop_newest"
          "#,
            BufferConfig::Memory {
                max_events: 500,
                when_full: WhenFull::DropNewest,
            },
        );

        #[cfg(feature = "disk-buffer")]
        check(
            r#"
          type = "disk"
          max_size = 1024
          "#,
            BufferConfig::Disk {
                max_size: 1024,
                when_full: WhenFull::Block,
            },
        );
    }
}
