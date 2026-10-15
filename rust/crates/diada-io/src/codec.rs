use crate::{DialogueNumbers, NumberFormatter, ReportError, ReportNumber};
use diada_compat::Real48;

/// The measured Turbo Pascal numeric adapter. No native floating point enters
/// parsing, accepted model data, report formatting, or angle conversions.
#[derive(Clone, Copy, Debug, Default)]
pub struct TpNumbers;
impl DialogueNumbers for TpNumbers {
    fn parse_real(&self, bytes: &[u8]) -> Result<Real48, String> {
        diada_compat::parse_real(bytes).map_err(|error| error.to_string())
    }
    fn scientific_real(&self, value: Real48) -> Result<Vec<u8>, String> {
        diada_compat::format_real(value, 0, None)
            .map(String::into_bytes)
            .map_err(|error| error.to_string())
    }
    fn degrees_to_radians(&self, value: Real48) -> Result<Real48, String> {
        diada_core::geometry::wrap_deg(value)
            .map(|(_, result)| result)
            .map_err(|error| error.to_string())
    }
    fn radians_to_degrees(&self, value: Real48) -> Result<Real48, String> {
        diada_core::geometry::wrap_rad(value)
            .map(|(_, result)| result)
            .map_err(|error| error.to_string())
    }
}
impl NumberFormatter for TpNumbers {
    fn format(
        &self,
        value: ReportNumber,
        width: i16,
        decimals: i16,
    ) -> Result<String, ReportError> {
        match value {
            ReportNumber::Real(value) => diada_compat::format_real(value, width, Some(decimals)),
            ReportNumber::Extended(value) => {
                diada_compat::format_extended(value, width, Some(decimals))
            }
        }
        .map_err(|error| ReportError::Numeric(error.to_string()))
    }
}
