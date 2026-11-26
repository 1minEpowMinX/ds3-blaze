//! Turning check results into what a step shows.

use crate::step::{Detail, Status};
use ds3_blaze_diag::netcheck::{Finding, Severity};
use ds3_blaze_diag::reach::{Report, Stage};

/// Returns the status `findings` give a step: a warning for a warning or a
/// problem, ok otherwise.
///
/// A problem makes a warning rather than a failure because the steps after it
/// still run; a step that stops them sets its failure itself.
pub fn status_of(findings: &[Finding]) -> Status {
    if findings
        .iter()
        .any(|finding| matches!(finding.severity, Severity::Warning | Severity::Problem))
    {
        Status::Warning
    } else {
        Status::Ok
    }
}

/// Returns `findings` as details.
pub fn details(findings: &[Finding]) -> Vec<Detail> {
    findings.iter().cloned().map(Detail::Finding).collect()
}

/// Returns a `reach` stage as a finding: ok when it passed, a warning for a
/// failed nickname stage, a problem for any other failure.
pub fn report_finding(report: &Report) -> Finding {
    let (severity, text) = match &report.result {
        Ok(text) => (Severity::Ok, text),
        Err(text) if report.stage == Stage::Nickname => (Severity::Warning, text),
        Err(text) => (Severity::Problem, text),
    };
    Finding {
        severity,
        text: format!("{}: {text}", report.stage),
        fix: None,
    }
}

/// Returns the first stage of `reports` other than the nickname stage that
/// failed.
pub fn first_failure(reports: &[Report]) -> Option<&Report> {
    reports
        .iter()
        .find(|report| report.stage != Stage::Nickname && report.result.is_err())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(severity: Severity) -> Finding {
        Finding {
            severity,
            text: String::new(),
            fix: None,
        }
    }

    #[test]
    fn warnings_and_problems_make_a_warning() {
        assert_eq!(
            status_of(&[finding(Severity::Ok), finding(Severity::Note)]),
            Status::Ok
        );
        assert_eq!(status_of(&[finding(Severity::Warning)]), Status::Warning);
        assert_eq!(status_of(&[finding(Severity::Problem)]), Status::Warning);
    }

    #[test]
    fn a_failed_nickname_is_a_warning_and_not_a_failure() {
        let nickname = Report {
            stage: Stage::Nickname,
            result: Err("refused".into()),
        };
        let blaze = Report {
            stage: Stage::Blaze,
            result: Err("no answer".into()),
        };
        assert_eq!(report_finding(&nickname).severity, Severity::Warning);
        assert_eq!(report_finding(&blaze).severity, Severity::Problem);
        assert_eq!(first_failure(std::slice::from_ref(&nickname)), None);
        assert_eq!(first_failure(&[nickname, blaze.clone()]), Some(&blaze));
    }
}
