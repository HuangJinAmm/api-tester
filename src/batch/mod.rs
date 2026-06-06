use std::path::Path;

use crate::{cli::RunArgs, error::Result};

pub async fn run_csv(path: &Path, args: &RunArgs) -> Result<()> {
    crate::csv_runner::run_csv(path, args).await
}
