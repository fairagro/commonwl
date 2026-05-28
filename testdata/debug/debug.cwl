#!/usr/bin/env cwl-runner

class: CommandLineTool
cwlVersion: v1.2

requirements:
- class: InitialWorkDirRequirement
  listing:
  - entryname: workflows/temp/temp.py
    entry:
      $include: workflows/temp/temp.py
- class: InlineJavascriptRequirement

inputs: []

outputs:
- id: catch_all
  type:
    type: array
    items:
    - 'null'
    - File
    - Directory
  outputBinding:
    glob: '*'
    outputEval: |-
      ${ var staged = ["workflows"]; return self.filter(function(f) { return staged.indexOf(f.basename) === -1; }); }

baseCommand:
- python3
- workflows/temp/temp.py
