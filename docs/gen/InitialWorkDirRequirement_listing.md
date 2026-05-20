The list of files or subdirectories that must be staged prior
to executing the command line tool.

Return type of each expression must validate as `["null",
File, Directory, Dirent, {type: array, items: [File,
Directory]}]`.

Each `File` or `Directory` that is returned by an Expression
must be added to the designated output directory prior to
executing the tool.

Each `Dirent` record that is listed or returned by an
expression specifies a file to be created or staged in the
designated output directory prior to executing the tool.

Expressions may return null, in which case they have no effect.

Files or Directories which are listed in the input parameters
and appear in the `InitialWorkDirRequirement` listing must
have their `path` set to their staged location.  If the same
File or Directory appears more than once in the
`InitialWorkDirRequirement` listing, the implementation must
choose exactly one value for `path`; how this value is chosen
is undefined.
