use super::*;

#[derive(Debug)]
pub struct YosysCapture {
    pub process: ProcessOutput,
    pub files: BTreeMap<PathBuf, Vec<u8>>,
}

impl YosysCapture {
    pub(super) fn validate_stages(&self, plan: &YosysPlan) -> Result<()> {
        for stage in plan.stages() {
            if !self.files.contains_key(Path::new(&format!("{stage}.log"))) {
                return Err(ExperimentError::Parse(format!(
                    "missing Yosys stage log {stage}"
                )));
            }
        }
        Ok(())
    }
}

impl YosysRun {
    pub fn decode(plan: &YosysPlan, captured: YosysCapture) -> Result<Self> {
        captured.validate_stages(plan)?;
        let stage_logs = plan
            .stages()
            .iter()
            .map(|name| {
                let bytes = &captured.files[Path::new(&format!("{name}.log"))];
                let text = std::str::from_utf8(bytes)
                    .map_err(|error| ExperimentError::Parse(error.to_string()))?;
                Ok((name.clone(), text.to_owned()))
            })
            .collect::<Result<_>>()?;
        let bytes = captured
            .files
            .get(Path::new("timing.json"))
            .ok_or_else(|| ExperimentError::Parse("missing Yosys pass timings".into()))?;
        let mut timings = YosysTimings::parse(bytes)?;
        if let Some(trace) = captured.files.get(Path::new("calls.json")) {
            timings = timings.with_call_trace(trace)?;
        }
        Ok(Self {
            process: captured.process,
            stage_logs,
            timings,
        })
    }
}
