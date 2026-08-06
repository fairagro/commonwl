#!/usr/bin/env cwl-runner

class: CommandLineTool
cwlVersion: v1.2

$namespaces:
  edam: http://edamontology.org/
$schemas:
  - https://edamontology.org/EDAM.owl

inputs:
  file1:
    type: File
    format: edam:format_2330

outputs:
  output:
    type: File
    outputBinding: { glob: output }

baseCommand: [cat]

stdin: $(inputs.file1.path)
stdout: output
