use std::time::SystemTime;

use anyhow::Error;

pub fn compute_time_diff(start_time: SystemTime) -> Result<usize, Error> {

    let computed_diff = SystemTime::now()
        .duration_since(start_time)?
        .as_millis()
        .try_into()?;

    Ok(computed_diff)
}
