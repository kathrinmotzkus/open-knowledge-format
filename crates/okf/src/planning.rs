use crate::{PlanningHeadings, PlanningSections};

#[derive(Clone, Copy)]
enum Section {
    None,
    Completed,
    Open,
    Deferred,
}

pub(crate) fn extract(body: &str, headings: &PlanningHeadings) -> PlanningSections {
    let mut current = Section::None;
    let mut sections = PlanningSections::default();

    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("## ") {
            current = classify(heading, headings);
            continue;
        }
        if !trimmed.starts_with("- ") && !trimmed.starts_with("* ") {
            continue;
        }
        match current {
            Section::Completed => sections.completed.push(trimmed.to_string()),
            Section::Open => sections.open.push(trimmed.to_string()),
            Section::Deferred => sections.deferred.push(trimmed.to_string()),
            Section::None => {}
        }
    }
    sections
}

fn classify(heading: &str, headings: &PlanningHeadings) -> Section {
    let heading = heading.trim();
    if contains_heading(&headings.completed, heading) {
        Section::Completed
    } else if contains_heading(&headings.open, heading) {
        Section::Open
    } else if contains_heading(&headings.deferred, heading) {
        Section::Deferred
    } else {
        Section::None
    }
}

fn contains_heading(headings: &[String], candidate: &str) -> bool {
    headings
        .iter()
        .any(|heading| heading.eq_ignore_ascii_case(candidate))
}
