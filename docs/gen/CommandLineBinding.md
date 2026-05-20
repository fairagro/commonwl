When listed under `inputBinding` in the input schema, the term
"value" refers to the corresponding value in the input object. For
binding objects listed in `CommandLineTool.arguments`, the term "value"
refers to the effective value after evaluating `valueFrom`.

The binding behavior when building the command line depends on the data
type of the value. If there is a mismatch between the type described by
the input schema and the effective value, such as resulting from an
expression evaluation, an implementation must use the data type of the
effective value.

- **string**: Add `prefix` and the string to the command line.

- **number**: Add `prefix` and decimal representation to command line.

- **boolean**: If true, add `prefix` to the command line. If false, add
  nothing.

- **`File`**: Add `prefix` and the value of
  [`File.path`](#File) to the command line.

- **`Directory`**: Add `prefix` and the value of
  [`Directory.path`](#Directory) to the command line.

- **array**: If `itemSeparator` is specified, add `prefix` and the join
  the array into a single string with `itemSeparator` separating the
  items. Otherwise, first add `prefix`, then recursively process
  individual elements.
  If the array is empty, it does not add anything to command line.

- **object**: Add `prefix` only, and recursively add object fields for
  which `inputBinding` is specified.

- **null**: Add nothing.
