use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::loop_runner::MemoryBankClient;

#[derive(Debug, Clone)]
pub struct FileMemoryBank {
    root: PathBuf,
}

impl FileMemoryBank {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("index.md")
    }

    fn architecture_path(&self) -> PathBuf {
        self.root.join("architecture/agent-loop.md")
    }

    fn guide_path(&self) -> PathBuf {
        self.root.join("guides/agent-loop.md")
    }
}

impl MemoryBankClient for FileMemoryBank {
    fn ensure_exists(&self) -> Result<()> {
        fs::create_dir_all(self.root.join("architecture"))?;
        fs::create_dir_all(self.root.join("guides"))?;

        if !self.index_path().exists() {
            fs::write(
                self.index_path(),
                "# Memory Bank\n\n- [architecture/agent-loop.md](architecture/agent-loop.md): loop architecture\n- [guides/agent-loop.md](guides/agent-loop.md): operational guide\n",
            )?;
        }

        if !self.architecture_path().exists() {
            fs::write(
                self.architecture_path(),
                "# Agent Loop Architecture\n\nSingle-run orchestrator with parent/child task model and hybrid done evaluator.\n",
            )?;
        }

        if !self.guide_path().exists() {
            fs::write(
                self.guide_path(),
                "# Agent Loop Guide\n\nRun `agent-loop run` to process one parent task end-to-end.\n",
            )?;
        }

        Ok(())
    }

    fn prime(&self) -> Result<()> {
        let _ = fs::read_to_string(self.index_path())?;
        Ok(())
    }

    fn prepare(&self, _topic: &str) -> Result<()> {
        let _ = fs::read_to_string(self.architecture_path())?;
        let _ = fs::read_to_string(self.guide_path())?;
        Ok(())
    }
}
