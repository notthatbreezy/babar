# WorkflowContext

Work Title: Typed Query Macros
Work ID: typed-query-macros
Base Branch: main
Target Branch: feature/typed-query-macros
Execution Mode: current-checkout
Repository Identity: github.com/notthatbreezy/babar@0810d71a7fb7be949ac3d18273b3fb33892c8fe1
Execution Binding: none
Workflow Mode: custom
Review Strategy: local
Review Policy: final-pr-only
Session Policy: continuous
Final Agent Review: enabled
Final Review Mode: single-model
Final Review Interactive: smart
Final Review Models: gpt-5.4
Final Review Specialists: all
Final Review Interaction Mode: parallel
Final Review Specialist Models: none
Final Review Perspectives: auto
Final Review Perspective Cap: 2
Implementation Model: none
Plan Generation Mode: single-model
Plan Generation Models: gpt-5.4
Planning Docs Review: enabled
Planning Review Mode: multi-model
Planning Review Interactive: smart
Planning Review Models: gpt-5.4, claude-opus-4.7, claude-sonnet-4.6
Planning Review Specialists: all
Planning Review Interaction Mode: parallel
Planning Review Specialist Models: none
Planning Review Perspectives: auto
Planning Review Perspective Cap: 2
Custom Workflow Instructions: PAW workflow for exploring a greenfield typed-query macro system built around pg_parse. Treat backwards compatibility as non-goal. Investigate the most promising stack for fully type-checked, schema-aware literal query macros in babar; produce planning artifacts and implementation details. Focus on type-safety and developer ergonomics. Think about query composition. Be willing to make compromises like always having to use full table qualifiers to reference fields (e.g. users.id, users.name, etc.). Think about error messages and being generally helpful, it has to be fun, performant, and productive.
Initial Prompt: Explore a greenfield typed-query macro system built around pg_parse for fully type-checked, schema-aware literal query macros in babar.
Issue URL: none
Remote: origin
Artifact Lifecycle: commit-and-clean
Artifact Paths: auto-derived
Additional Inputs: none
