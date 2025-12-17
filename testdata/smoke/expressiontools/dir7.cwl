#!/usr/bin/env cwl-runner

cwlVersion: v1.2
class: ExpressionTool

requirements:
  InlineJavascriptRequirement: {}

inputs:
  files: File[]

outputs:
  dir: Directory
expression: |-
  ${
  return {"dir": {"class": "Directory", "basename": "a_directory", "listing": inputs.files}};
  }
