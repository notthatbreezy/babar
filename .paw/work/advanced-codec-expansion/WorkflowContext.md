# WorkflowContext

Work Title: Advanced Codec Expansion
Work ID: advanced-codec-expansion
Base Branch: main
Target Branch: feature/advanced-codec-expansion
Execution Mode: current-checkout
Repository Identity: none
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
Custom Workflow Instructions: PAW Lite workflow. Use .paw/work/advanced-codec-expansion/Plan.md as the plan artifact. Focus on PostGIS support with geo/geo-types as the primary codec expansion direction; include pgvector, macaddr/macaddr8, bit/varbit, multirange, tsvector, hstore, and citext in the same plan. Do not plan PostgreSQL built-in geometric types as part of this work.
Initial Prompt: ok - plan an update that follows how you laid it out; focus on postgis support with geo rust/geo-types and ignore geometric (that is the wrong path for most people). I agree on pgvector.
Issue URL: none
Remote: origin
Artifact Lifecycle: commit-and-clean
Artifact Paths: auto-derived
Additional Inputs: none
