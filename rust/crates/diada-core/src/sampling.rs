//! Calculation positions are independent from the UI repaint frequency.
//! Each input follows its own original six-byte stored increment.
use crate::expression::r;
use crate::*;

pub struct KinematicSampler {
    mechanism: Mechanism,
    steps: Vec<InputPosition>,
    increments: Vec<Real48>,
    position_index: usize,
    remaining: usize,
    mode: AnalysisMode,
    pending_increment_error: Option<CoreError>,
    last_kinematics_succeeded: bool,
}
impl KinematicSampler {
    /// Current partially computed state for display of already defined leading
    /// groups after a failed frame. Downstream objects may hold prior values;
    /// this is never a calculation snapshot or a report input.
    pub fn preview_mechanism(&self) -> &Mechanism {
        &self.mechanism
    }
    /// Display metadata for the last attempted frame. A force failure can
    /// leave a complete defined pose even though no full analysis snapshot is
    /// returned. This flag never certifies the reaction fields.
    pub fn last_kinematics_succeeded(&self) -> bool {
        self.last_kinematics_succeeded
    }
    pub fn new(mechanism: &Mechanism, config: &AnalysisConfig) -> Result<Self, CoreError> {
        if config.inputs.len() != mechanism.inputs.len() {
            return Err(CoreError::InvalidAnalysis(
                "all input links require a series",
            ));
        }
        let mut steps = Vec::new();
        let mut increments = Vec::new();
        for id in &mechanism.inputs {
            let series = config
                .inputs
                .iter()
                .find(|s| s.link == *id)
                .ok_or(CoreError::InvalidAnalysis("input series missing"))?;
            steps.push(InputPosition {
                position: series.start,
                other_position: series.other_start,
                rate: series.rate,
                rate_derivative: series.rate_derivative,
            });
            increments.push(series.step);
        }
        Ok(Self {
            mechanism: mechanism.clone(),
            steps,
            increments,
            position_index: 0,
            remaining: config.positions,
            mode: config.mode,
            pending_increment_error: None,
            last_kinematics_succeeded: false,
        })
    }
    /// U05 AnalysisDone increments inputs after WriteResults and ShowPlans
    /// consume the current position. Defer a failed increment to the next
    /// requested position so it cannot replace the result just calculated.
    fn advance_inputs(&mut self) {
        if self.remaining == 0 {
            return;
        }
        let next: Result<Vec<_>, CoreError> = self
            .steps
            .iter()
            .zip(&self.increments)
            .map(|(step, increment)| (r(step.position) + r(*increment)).real())
            .collect();
        match next {
            Ok(positions) => {
                for (step, position) in self.steps.iter_mut().zip(positions) {
                    step.position = position;
                }
            }
            Err(error) => self.pending_increment_error = Some(error),
        }
    }
    fn take_increment_error(&mut self) -> Option<CoreError> {
        let error = self.pending_increment_error.take()?;
        // The next input is undefined, so the series ends after reporting the
        // error once. In particular, never recalculate the prior input value.
        self.remaining = 0;
        Some(error)
    }
    pub fn next_snapshot(&mut self) -> Result<Option<CalculationSnapshot>, CoreError> {
        self.last_kinematics_succeeded = false;
        if let Some(error) = self.take_increment_error() {
            return Err(error);
        }
        if self.remaining == 0 {
            return Ok(None);
        }
        self.mechanism.calculate_position(&self.steps)?;
        self.last_kinematics_succeeded = true;
        if self.mode == AnalysisMode::Kinetostatics {
            self.mechanism.calculate_forces()?;
        }
        let snapshot = CalculationSnapshot {
            position_index: self.position_index,
            inputs: self.steps.clone(),
            mechanism: self.mechanism.clone(),
        };
        self.remaining -= 1;
        self.position_index += 1;
        self.advance_inputs();
        Ok(Some(snapshot))
    }
    /// Motion and graph positions advance after an explicitly failed position.
    /// The UI receives a gap, while subsequent input positions remain defined.
    pub fn next_frame(&mut self) -> Option<Result<CalculationSnapshot, CoreError>> {
        self.last_kinematics_succeeded = false;
        if let Some(error) = self.take_increment_error() {
            return Some(Err(error));
        }
        if self.remaining == 0 {
            return None;
        }
        let result = self
            .mechanism
            .calculate_position(&self.steps)
            .and_then(|()| {
                self.last_kinematics_succeeded = true;
                if self.mode == AnalysisMode::Kinetostatics {
                    self.mechanism.calculate_forces()?;
                }
                Ok(CalculationSnapshot {
                    position_index: self.position_index,
                    inputs: self.steps.clone(),
                    mechanism: self.mechanism.clone(),
                })
            });
        self.remaining -= 1;
        self.position_index += 1;
        self.advance_inputs();
        Some(result)
    }
}
impl Mechanism {
    pub fn sample_kinematics(
        &self,
        config: &AnalysisConfig,
    ) -> Result<Vec<CalculationSnapshot>, CoreError> {
        let mut sampler = KinematicSampler::new(self, config)?;
        let mut snapshots = Vec::new();
        while let Some(snapshot) = sampler.next_snapshot()? {
            snapshots.push(snapshot);
        }
        Ok(snapshots)
    }
    pub fn sample_frames(
        &self,
        config: &AnalysisConfig,
    ) -> Result<Vec<Result<CalculationSnapshot, CoreError>>, CoreError> {
        let mut sampler = KinematicSampler::new(self, config)?;
        let mut frames = Vec::new();
        while let Some(frame) = sampler.next_frame() {
            frames.push(frame);
        }
        Ok(frames)
    }
}
