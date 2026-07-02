use crate::AcceptedSuggestion;
use std::collections::BTreeSet;
use std::fmt;

pub(crate) fn relation_from_suggestion(s: &AcceptedSuggestion) -> serde_json::Value {
    serde_json::json!({"type":"ai_suggested_edge","target":s.target_document,"source_chunk":s.source_chunk,"target_chunk":s.target_chunk,"suggestion_id":s.id,"provider":s.provider,"model":s.model,"generation_method":s.generation_method,"ai_generated":s.ai_generated,"score":s.score,"created_at":s.created_at,"status":"accepted"})
}

#[derive(Debug)]
pub(crate) struct RelationMerge {
    pub(crate) source: String,
    pub(crate) added_ids: BTreeSet<String>,
    pub(crate) existing_ids: BTreeSet<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum RelationEditError {
    UnterminatedFrontmatter,
    InvalidRelations(String),
}

impl fmt::Display for RelationEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedFrontmatter => formatter.write_str("frontmatter is not terminated"),
            Self::InvalidRelations(error) => {
                write!(formatter, "relations metadata is invalid: {error}")
            }
        }
    }
}

pub(crate) fn merge_relations_into_frontmatter(
    source: &str,
    new_relations: Vec<serde_json::Value>,
) -> Result<RelationMerge, RelationEditError> {
    let document = FrontmatterDocument::parse(source)?;
    let relation_field = document.field("relations");
    let mut existing = match relation_field.as_ref() {
        Some(field) => parse_relations_value(&field.value)?,
        None => Vec::new(),
    };
    let mut ids = existing
        .iter()
        .filter_map(relation_id)
        .collect::<BTreeSet<_>>();
    let mut added_ids = BTreeSet::new();
    let mut existing_ids = BTreeSet::new();
    for relation in new_relations {
        let Some(id) = relation_id(&relation) else {
            return Err(RelationEditError::InvalidRelations(
                "every relation requires a non-empty suggestion_id".to_string(),
            ));
        };
        if ids.contains(&id) {
            existing_ids.insert(id);
            continue;
        }
        ids.insert(id.clone());
        added_ids.insert(id);
        existing.push(relation);
    }
    let rendered = render_relations(&existing).map_err(RelationEditError::InvalidRelations)?;
    Ok(RelationMerge {
        source: document.replace_or_append(relation_field.as_ref(), &rendered),
        added_ids,
        existing_ids,
    })
}

fn relation_id(relation: &serde_json::Value) -> Option<String> {
    relation
        .get("suggestion_id")?
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn parse_relations_value(value: &str) -> Result<Vec<serde_json::Value>, RelationEditError> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    let parsed = serde_json::from_str::<Vec<serde_json::Value>>(value)
        .map_err(|error| RelationEditError::InvalidRelations(error.to_string()))?;
    if parsed.iter().any(|relation| !relation.is_object()) {
        return Err(RelationEditError::InvalidRelations(
            "relations must be JSON objects".to_string(),
        ));
    }
    Ok(parsed)
}

fn render_relations(relations: &[serde_json::Value]) -> Result<String, String> {
    let json = serde_json::to_string_pretty(relations).map_err(|error| error.to_string())?;
    let mut lines = json.lines();
    let first = lines.next().unwrap_or("[]");
    let mut rendered = format!("relations: {first}");
    for line in lines {
        rendered.push('\n');
        rendered.push_str("  ");
        rendered.push_str(line);
    }
    Ok(rendered)
}

#[derive(Debug)]
struct FrontmatterDocument<'a> {
    source: &'a str,
    header_start: usize,
    header_end: usize,
    newline: &'static str,
    had_frontmatter: bool,
}

#[derive(Debug)]
struct FrontmatterField {
    start: usize,
    end: usize,
    value: String,
}

impl<'a> FrontmatterDocument<'a> {
    fn parse(source: &'a str) -> Result<Self, RelationEditError> {
        let Some(opening_end) = source.find('\n').map(|index| index + 1) else {
            return Ok(Self::without_frontmatter(source));
        };
        if source[..opening_end].trim_end_matches(['\r', '\n']) != "---" {
            return Ok(Self::without_frontmatter(source));
        }
        let newline = if source[..opening_end].ends_with("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut offset = opening_end;
        for segment in source[opening_end..].split_inclusive('\n') {
            if segment.trim_end_matches(['\r', '\n']) == "---" {
                return Ok(Self {
                    source,
                    header_start: opening_end,
                    header_end: offset,
                    newline,
                    had_frontmatter: true,
                });
            }
            offset += segment.len();
        }
        Err(RelationEditError::UnterminatedFrontmatter)
    }

    fn without_frontmatter(source: &'a str) -> Self {
        Self {
            source,
            header_start: 0,
            header_end: 0,
            newline: "\n",
            had_frontmatter: false,
        }
    }

    fn field(&self, wanted: &str) -> Option<FrontmatterField> {
        if !self.had_frontmatter {
            return None;
        }
        let header = &self.source[self.header_start..self.header_end];
        let mut fields = Vec::<(String, usize, usize)>::new();
        let mut offset = 0;
        for segment in header.split_inclusive('\n') {
            let line = segment.trim_end_matches(['\r', '\n']);
            if !line.starts_with(' ') && !line.starts_with('\t') {
                if let Some((key, _)) = line.split_once(':') {
                    let key = key.trim();
                    if !key.is_empty()
                        && key.chars().all(|character| {
                            character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                        })
                    {
                        fields.push((key.to_string(), offset, header.len()));
                        if fields.len() > 1 {
                            let previous = fields.len() - 2;
                            fields[previous].2 = offset;
                        }
                    }
                }
            }
            offset += segment.len();
        }
        let (_, start, end) = fields.into_iter().find(|(key, _, _)| key == wanted)?;
        let span = &header[start..end];
        let first_end = span.find('\n').unwrap_or(span.len());
        let first = span[..first_end].trim_end_matches('\r');
        let (_, first_value) = first.split_once(':')?;
        let continuation = &span[first_end..];
        let value = if continuation.is_empty() {
            first_value.trim().to_string()
        } else {
            format!("{}{}", first_value.trim(), continuation)
                .trim()
                .to_string()
        };
        Some(FrontmatterField {
            start: self.header_start + start,
            end: self.header_start + end,
            value,
        })
    }

    fn replace_or_append(&self, field: Option<&FrontmatterField>, rendered: &str) -> String {
        if let Some(field) = field {
            let mut output = String::with_capacity(self.source.len() + rendered.len());
            output.push_str(&self.source[..field.start]);
            output.push_str(&rendered.replace('\n', self.newline));
            output.push_str(self.newline);
            output.push_str(&self.source[field.end..]);
            return output;
        }
        if self.had_frontmatter {
            let mut output = String::with_capacity(self.source.len() + rendered.len());
            output.push_str(&self.source[..self.header_end]);
            if self.header_end > self.header_start
                && !self.source[..self.header_end].ends_with(self.newline)
            {
                output.push_str(self.newline);
            }
            output.push_str(&rendered.replace('\n', self.newline));
            output.push_str(self.newline);
            output.push_str(&self.source[self.header_end..]);
            return output;
        }
        format!(
            "---{nl}{rendered}{nl}---{nl}{nl}{}",
            self.source,
            nl = self.newline
        )
    }
}
