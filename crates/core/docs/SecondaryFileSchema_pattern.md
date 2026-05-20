Provides a pattern or expression specifying files or directories that
should be included alongside the primary file.

If the value is an expression, the value of `self` in the
expression must be the primary input or output `File` object to
which this binding applies. The `basename`, `nameroot` and
`nameext` fields must be present in `self`. For
`CommandLineTool` inputs the `location` field must also be
present. For `CommandLineTool` outputs the `path` field must
also be present. If secondary files were included on an input
`File` object as part of the Process invocation, they must also
be present in `secondaryFiles` on `self`.

The expression must return either: a filename string relative
to the path to the primary `File`, a `File` or `Directory` object
(`class: File` or `class: Directory`) with either `location`
(for inputs) or `path` (for outputs) and `basename` fields
set, or an array consisting of strings or `File` or `Directory`
objects as previously described.

It is legal to use `location` from a `File` or `Directory` object
passed in as input, including `location` from secondary files
on `self`. If an expression returns a `File` object with the
same `location` but a different `basename` as a secondary file
that was passed in, the expression result takes precedence.
Setting the basename with an expression this way affects the
`path` where the secondary file will be staged to in the
`CommandLineTool`.

The expression may return "null" in which case there is no
secondary file from that expression.

To work on non-filename-preserving storage systems, portable
tool descriptions should treat `location` as an
[opaque identifier](#opaque-strings) and avoid constructing new
values from `location`, but should construct relative references
using `basename` or `nameroot` instead, or propagate `location`
from defined inputs.

If a value in `secondaryFiles` is a string that is not an expression,
it specifies that the following pattern should be applied to the path
of the primary file to yield a filename relative to the primary `File`:

1. If string ends with `?` character, remove the last `?` and mark
   the resulting secondary file as optional.
1. If string begins with one or more caret `^` characters, for each
   caret, remove the last file extension from the path (the last
   period `.` and all following characters). If there are no file
   extensions, the path is unchanged.
1. Append the remainder of the string to the end of the file path.
