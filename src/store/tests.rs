use crate::experiment::{BenchmarkIn, BenchmarkOut};

use super::*;

// for now, think of this
#[derive(Debug, Clone)]
pub struct FlatExpResults {
    results: HashMap<ExperimentId, String>,
}

impl Store for FlatExpResults {
    fn get_results<BMI, BMO>(
        &self,
        exp_id: ExperimentId,
        commit_id: CommitId,
    ) -> Result<SuiteResults<BMI, BMO>, StoreError>
    where
        BMI: BenchmarkIn,
        BMO: BenchmarkOut,
    {
        let result: &String = self
            .results
            .get(&exp_id)
            .ok_or(StoreError::ExperimentIdNotFound(exp_id))?;
        let exp_res: ExperimentResults<BMI, BMO> = serde_json::from_str(result)?;
        let suite_res: SuiteResults<BMI, BMO> = exp_res
            .suites
            .get(&commit_id)
            .ok_or(StoreError::CommitIdNotFound(commit_id))?
            .clone();
        Ok(suite_res)
    }
}
