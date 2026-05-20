Represents a file (or group of files when `secondaryFiles` is provided) that
will be accessible by tools using standard POSIX file system call API such as
open(2) and read(2).

Files are represented as objects with `class` of `File`.  File objects have
a number of properties that provide metadata about the file.

The `location` property of a File is a IRI that uniquely identifies the
file.  Implementations must support the `file://` IRI scheme and may support
other schemes such as `http://` and `https://`.  The value of `location` may also be a
relative reference, in which case it must be resolved relative to the IRI
of the document it appears in.  Alternately to `location`, implementations
must also accept the `path` property on File, which must be a filesystem
path available on the same host as the CWL runner (for inputs) or the
runtime environment of a command line tool execution (for command line tool
outputs).

If no `location` or `path` is specified, a file object must specify
`contents` with the UTF-8 text content of the file.  This is a "file
literal".  File literals do not correspond to external resources, but are
created on disk with `contents` with when needed for executing a tool.
Where appropriate, expressions can return file literals to define new files
on a runtime.  The maximum size of `contents` is 64 kilobytes.

The `basename` property defines the filename on disk where the file is
staged.  This may differ from the resource name.  If not provided,
`basename` must be computed from the last path part of `location` and made
available to expressions.

The `secondaryFiles` property is a list of File or Directory objects that
must be staged in the same directory as the primary file.  It is an error
for file names to be duplicated in `secondaryFiles`.

The `size` property is the size in bytes of the File.  It must be computed
from the resource and made available to expressions.  The `checksum` field
contains a cryptographic hash of the file content for use it verifying file
contents.  Implementations may, at user option, enable or disable
computation of the `checksum` field for performance or other reasons.
However, the ability to compute output checksums is required to pass the
CWL conformance test suite.

When executing a CommandLineTool, the files and secondary files may be
staged to an arbitrary directory, but must use the value of `basename` for
the filename.  The `path` property must be file path in the context of the
tool execution runtime (local to the compute node, or within the executing
container).  All computed properties should be available to expressions.
File literals also must be staged and `path` must be set.

When collecting CommandLineTool outputs, `glob` matching returns file paths
(with the `path` property) and the derived properties. This can all be
modified by `outputEval`.  Alternately, if the file `cwl.output.json` is
present in the output, `outputBinding` is ignored.

File objects in the output must provide either a `location` IRI or a `path`
property in the context of the tool execution runtime (local to the compute
node, or within the executing container).

When evaluating an ExpressionTool, file objects must be referenced via
`location` (the expression tool does not have access to files on disk so
`path` is meaningless) or as file literals.  It is legal to return a file
object with an existing `location` but a different `basename`.  The
`loadContents` field of ExpressionTool inputs behaves the same as on
CommandLineTool inputs, however it is not meaningful on the outputs.

An ExpressionTool may forward file references from input to output by using
the same value for `location`.
