use std::collections::HashSet;
use std::sync::LazyLock;

pub static CWL_CLASSES: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["Workflow", "CommandLineTool", "ExpressionTool", "Operation"]));

pub static CWL_CORE_FIELDS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["class", "cwlVersion", "id", "label", "doc", "intent"]));

pub static CWL_IO_FIELDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "inputs",
        "outputs",
        "type",
        "default",
        "format",
        "streamable",
        "secondaryFiles",
        "inputBinding",
        "outputBinding",
        "loadContents",
        "loadListing",
        "valueFrom",
    ])
});

pub static CWL_WORKFLOW_FIELDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "steps",
        "run",
        "in",
        "out",
        "scatter",
        "scatterMethod",
        "when",
        "pickValue",
        "linkMerge",
    ])
});

pub static CWL_COMMAND_FIELDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "baseCommand",
        "arguments",
        "stdin",
        "stdout",
        "stderr",
        "successCodes",
        "temporaryFailCodes",
        "permanentFailCodes",
        "shellQuote",
        "position",
        "prefix",
        "separate",
        "itemSeparator",
    ])
});

pub static CWL_REQUIREMENTS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "DockerRequirement",
        "EnvVarRequirement",
        "ExpressionEngineRequirement",
        "InitialWorkDirRequirement",
        "InlineJavascriptRequirement",
        "InplaceUpdateRequirement",
        "LoadListingRequirement",
        "MPIRequirement",
        "MultipleInputFeatureRequirement",
        "NetworkAccess",
        "ProcessGenerator",
        "ResourceRequirement",
        "ScatterFeatureRequirement",
        "SchemaDefRequirement",
        "ShellCommandRequirement",
        "SoftwareRequirement",
        "StepInputExpressionRequirement",
        "SubworkflowFeatureRequirement",
        "ToolTimeLimit",
        "WorkReuse",
    ])
});

pub static CWL_HINT_FIELDS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["requirements", "hints"]));

pub static CWL_PRIMITIVE_TYPES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "null",
        "boolean",
        "int",
        "long",
        "float",
        "double",
        "string",
        "File",
        "Directory",
        "enum",
        "record",
        "array",
    ])
});

pub static CWL_SCATTER_METHODS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["dotproduct", "nested_crossproduct", "flat_crossproduct"]));

pub static CWL_LINK_MERGE: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["merge_nested", "merge_flattened"]));

pub static CWL_PICK_VALUE: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["first_non_null", "the_only_non_null", "all_non_null"]));
