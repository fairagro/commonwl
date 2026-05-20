Find files or directories relative to the output directory, using POSIX
glob(3) pathname matching. If an array is provided, find files or
directories that match any pattern in the array. If an expression is
provided, the expression must return a string or an array of strings,
which will then be evaluated as one or more glob patterns. Must only
match and return files/directories which actually exist.

If the value of glob is a relative path pattern (does not
begin with a slash '/') then it is resolved relative to the
output directory. If the value of the glob is an absolute
path pattern (it does begin with a slash '/') then it must
refer to a path within the output directory. It is an error
if any glob resolves to a path outside the output directory.
Specifically this means globs that resolve to paths outside the output
directory are illegal.

A glob may match a path within the output directory which is
actually a symlink to another file. In this case, the
expected behavior is for the resulting `File`/Directory object to take the
`basename` (and corresponding `nameroot` and `nameext`) of the
symlink. The `location` of the `File`/Directory is implementation
dependent, but logically the `File`/Directory should have the same content
as the symlink target. Platforms may stage output files/directories to
cloud storage that lack the concept of a symlink. In
this case file content and directories may be duplicated, or (to avoid
duplication) the `File`/Directory `location` may refer to the symlink
target.

It is an error if a symlink in the output directory (or any
symlink in a chain of links) refers to any file or directory
that is not under an input or output directory.

Implementations may shut down a container before globbing
output, so globs and expressions must not assume access to the
container filesystem except for declared input and output.
