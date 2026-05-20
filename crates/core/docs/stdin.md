Only valid as a `type` for a `CommandLineTool` input with no
`inputBinding` set. `stdin` must not be specified at the `CommandLineTool`
level.

The following

```
inputs:
   an_input_name:
   type: stdin
```

is equivalent to

```
inputs:
  an_input_name:
    type: File
    streamable: true

stdin: $(inputs.an_input_name.path)
```
