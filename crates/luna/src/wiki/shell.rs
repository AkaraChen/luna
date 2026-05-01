use bashkit::{Bash, ExecResult, InMemoryFs};

use crate::error::Result;

/// Wrapper around a bashkit [`Bash`] instance pre-configured with a wiki filesystem.
pub struct WikiShell {
    bash: Bash,
}

impl WikiShell {
    pub fn new(fs: InMemoryFs) -> Self {
        let bash = Bash::builder()
            .fs(std::sync::Arc::new(fs))
            .cwd("/")
            .build();
        Self { bash }
    }

    pub async fn exec(&mut self, command: &str) -> Result<ExecResult> {
        Ok(self.bash.exec(command).await?)
    }
}
