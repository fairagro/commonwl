#!/usr/bin/env cwl-runner

cwlVersion: v1.2
class: ExpressionTool

requirements:
  InlineJavascriptRequirement: {}
  ToolTimeLimit:
    timelimit: 3

inputs: []

outputs:
  status: string
expression: |-
  ${
    function sleep(milliseconds) {
      var start = new Date().getTime();
      for (var i = 0; i < 1e7; i++) {
        if ((new Date().getTime() - start) > milliseconds){
          break;
        }
      }
    };
    sleep(5000);
    return {"status": "Done"}
  }
