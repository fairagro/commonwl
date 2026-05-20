Represents a directory to present to a command line tool.

Directories are represented as objects with `class` of `Directory`. `Directory` objects have
a number of properties that provide metadata about the directory.

The `location` property of a `Directory` is a IRI that uniquely identifies
the directory. Implementations must support the file:// IRI scheme and may
support other schemes such as <http://.> Alternately to `location`,
implementations must also accept the `path` property on `Directory`, which
must be a filesystem path available on the same host as the CWL runner (for
inputs) or the runtime environment of a command line tool execution (for
command line tool outputs).

A `Directory` object may have a `listing` field. This is a list of `File` and
`Directory` objects that are contained in the `Directory`. For each entry in
`listing`, the `basename` property defines the name of the `File` or
Subdirectory when staged to disk. If `listing` is not provided, the
implementation must have some way of fetching the `Directory` listing at
runtime based on the `location` field.

If a `Directory` does not have `location`, it is a `Directory` literal. A
`Directory` literal must provide `listing`. `Directory` literals must be
created on disk at runtime as needed.

The resources in a `Directory` literal do not need to have any implied
relationship in their `location`. For example, a `Directory` listing may
contain two files located on different hosts. It is the responsibility of
the runtime to ensure that those files are staged to disk appropriately.
Secondary files associated with files in `listing` must also be staged to
the same `Directory`.

When executing a `CommandLineTool`, Directories must be recursively staged
first and have local values of `path` assigned.

`Directory` objects in `CommandLineTool` output must provide either a
`location` IRI or a `path` property in the context of the tool execution
runtime (local to the compute node, or within the executing container).

An `ExpressionTool` may forward file references from input to output by using
the same value for `location`.

Name conflicts (the same `basename` appearing multiple times in `listing`
or in any entry in `secondaryFiles` in the listing) is a fatal error.
