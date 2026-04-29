# WorkflowContext

Work Title: External Schemas
Work ID: external-schemas
Base Branch: main
Target Branch: feature/external-schemas
Execution Mode: current-checkout
Repository Identity: github.com/notthatbreezy/babar@0810d71a7fb7be949ac3d18273b3fb33892c8fe1
Execution Binding: none
Workflow Mode: full
Review Strategy: local
Review Policy: milestones
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
Custom Workflow Instructions: none
Initial Prompt: Plan support for external schema sources so typed_query! can use authored or generated schemas outside the inline schema DSL. I want to avoid a bunch of craziness when it comes defining the tables, it should feel as close to rust as possible - for instance, leveraging types to signal primary keys or other important attributes similar to how pydantic utilizes type annotations. We're focused on developer ergonomics here and keeping the external, user API as simple as possible. Definitely include research on how pydantic handles field definitions and type annotations. When appropriate, present a few options for for the rust API we're building.
Issue URL: none
Remote: origin
Artifact Lifecycle: commit-and-clean
Artifact Paths: auto-derived
Additional Inputs: none
