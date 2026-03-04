use cwl_core::{
    OneOrMany,
    inputs::{
        InputArraySchema, InputEnumSchema, InputRecordField, InputRecordSchema, InputSchema,
        InputType,
    },
    outputs::{
        OutputArraySchema, OutputEnumSchema, OutputRecordField, OutputRecordSchema, OutputSchema,
        OutputType,
    },
    types::CWLType,
};

pub(crate) enum ValidationType {
    CWLType(CWLType),
    Schema(Box<ValidationSchema>),
    #[allow(unused)]
    String(String),
}

pub(crate) enum ValidationSchema {
    Array(ValidationArraySchema),
    Enum(ValidationEnumSchema),
    Record(ValidationRecordSchema),
}

pub(crate) struct ValidationRecordField {
    pub name: String,
    pub r#type: OneOrMany<ValidationType>,
    pub format: Option<OneOrMany<String>>,
}

pub(crate) struct ValidationRecordSchema {
    pub fields: Option<Vec<ValidationRecordField>>,
}

pub(crate) struct ValidationEnumSchema {
    pub symbols: Vec<String>,
}

pub(crate) struct ValidationArraySchema {
    pub items: OneOrMany<ValidationType>,
}

impl From<InputType> for ValidationType {
    fn from(input_type: InputType) -> Self {
        match input_type {
            InputType::CWLType(ty) => ValidationType::CWLType(ty),
            InputType::InputSchema(schema) => ValidationType::Schema(Box::new((*schema).into())),
            InputType::String(format) => ValidationType::String(format),
        }
    }
}

impl From<InputSchema> for ValidationSchema {
    fn from(schema: InputSchema) -> Self {
        match schema {
            InputSchema::Array(array_schema) => ValidationSchema::Array(array_schema.into()),
            InputSchema::Enum(enum_schema) => ValidationSchema::Enum(enum_schema.into()),
            InputSchema::Record(record_schema) => ValidationSchema::Record(record_schema.into()),
        }
    }
}

impl From<InputArraySchema> for ValidationArraySchema {
    fn from(schema: InputArraySchema) -> Self {
        ValidationArraySchema {
            items: schema.items.map(|m| m.into()),
        }
    }
}

impl From<InputRecordSchema> for ValidationRecordSchema {
    fn from(schema: InputRecordSchema) -> Self {
        ValidationRecordSchema {
            fields: schema
                .fields
                .map(|v| v.into_iter().map(|f| f.into()).collect()),
        }
    }
}

impl From<InputRecordField> for ValidationRecordField {
    fn from(field: InputRecordField) -> Self {
        ValidationRecordField {
            name: field.name,
            r#type: field.r#type.map(|t| t.into()),
            format: field.format,
        }
    }
}

impl From<InputEnumSchema> for ValidationEnumSchema {
    fn from(schema: InputEnumSchema) -> Self {
        ValidationEnumSchema {
            symbols: schema.symbols,
        }
    }
}

impl From<OutputType> for ValidationType {
    fn from(output_type: OutputType) -> Self {
        match output_type {
            OutputType::CWLType(ty) => ValidationType::CWLType(ty),
            OutputType::OutputSchema(schema) => ValidationType::Schema(Box::new((*schema).into())),
            OutputType::String(format) => ValidationType::String(format),
        }
    }
}

impl From<OutputSchema> for ValidationSchema {
    fn from(schema: OutputSchema) -> Self {
        match schema {
            OutputSchema::Array(array_schema) => ValidationSchema::Array(array_schema.into()),
            OutputSchema::Enum(enum_schema) => ValidationSchema::Enum(enum_schema.into()),
            OutputSchema::Record(record_schema) => ValidationSchema::Record(record_schema.into()),
        }
    }
}

impl From<OutputArraySchema> for ValidationArraySchema {
    fn from(schema: OutputArraySchema) -> Self {
        ValidationArraySchema {
            items: schema.items.map(|m| m.into()),
        }
    }
}

impl From<OutputRecordSchema> for ValidationRecordSchema {
    fn from(schema: OutputRecordSchema) -> Self {
        ValidationRecordSchema {
            fields: schema
                .fields
                .map(|v| v.into_iter().map(|f| f.into()).collect()),
        }
    }
}

impl From<OutputRecordField> for ValidationRecordField {
    fn from(field: OutputRecordField) -> Self {
        ValidationRecordField {
            name: field.name,
            r#type: field.r#type.map(|t| t.into()),
            format: field.format,
        }
    }
}

impl From<OutputEnumSchema> for ValidationEnumSchema {
    fn from(schema: OutputEnumSchema) -> Self {
        ValidationEnumSchema {
            symbols: schema.symbols,
        }
    }
}
