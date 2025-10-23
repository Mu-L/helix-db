use core::fmt;
use std::fmt::Display;

use super::utils::GenRef;

pub struct ReturnValue {
    pub name: String,
    pub fields: Vec<ReturnValueField>,
    pub literal_value: Option<GenRef<String>>, // For literal return values like RETURN "Success"
}

impl Display for ReturnValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "#[derive(Serialize)]")?;
        writeln!(f, "pub struct {} {{", self.name)?;
        for field in &self.fields {
            writeln!(f, "    pub {}: {},", field.name, field.field_type)?;
        }
        writeln!(f, "}}")?;
        Ok(())
    }
}

pub struct ReturnValueField {
    pub name: String,
    pub field_type: String,
}