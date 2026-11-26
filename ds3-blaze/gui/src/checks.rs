//! Turning check results into what a step shows.

use crate::banner;
use crate::step::{Detail, Status, Step};
use ds3_blaze_diag::netcheck::{Finding, Severity};
use ds3_blaze_diag::reach::{Report, Stage};

/// Returns `findings` as details.
pub fn details(findings: &[Finding]) -> Vec<Detail> {
    findings.iter().cloned().map(Detail::Finding).collect()
}

/// Returns `text` as a sentence: its first letter in upper case.
pub fn sentence(text: &str) -> String {
    let mut letters = text.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// Puts `findings` at the head of `step`'s details and, unless the step runs
/// or ended well, makes the number of issues among them its summary and the
/// step a warning.
///
/// A step that failed moves its summary into the details, after the
/// findings.
pub fn add_findings(step: &mut Step, findings: &[Finding]) {
    let mut lines = details(findings);
    lines.append(&mut step.details);
    step.details = lines;
    let issues = findings
        .iter()
        .filter(|finding| matches!(finding.severity, Severity::Warning | Severity::Problem))
        .count();
    if issues == 0 || matches!(step.status, Status::Running | Status::Ok) {
        return;
    }
    if step.status == Status::Failed {
        let failure = std::mem::take(&mut step.summary);
        step.details.insert(findings.len(), Detail::Text(failure));
    }
    // The issues name what to change, so the step asks for a fix rather
    // than reporting a failure.
    step.status = Status::Warning;
    step.summary = banner::things_to_fix(issues);
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
    fn issues_lead_a_step_that_waits_or_failed() {
        let findings = [finding(Severity::Ok), finding(Severity::Warning)];
        let mut waiting = Step::new("Direct link");
        add_findings(&mut waiting, &findings);
        assert_eq!(waiting.status, Status::Warning);
        assert_eq!(waiting.summary, "1 thing to fix on this PC");
        assert_eq!(waiting.details.len(), 2);

        let mut failed = Step::new("Direct link");
        failed.status = Status::Failed;
        failed.summary = "no answer".into();
        add_findings(&mut failed, &findings);
        assert_eq!(failed.status, Status::Warning);
        assert_eq!(failed.details[2], Detail::Text("no answer".into()));

        let mut working = Step::new("Direct link");
        working.status = Status::Ok;
        working.summary = "both ways".into();
        add_findings(&mut working, &findings);
        assert_eq!(working.summary, "both ways");
        assert_eq!(working.details.len(), 2);
    }

    #[test]
    fn a_sentence_starts_with_a_capital() {
        assert_eq!(
            sentence("the host sees you as Carver"),
            "The host sees you as Carver"
        );
        assert_eq!(sentence(""), "");
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
