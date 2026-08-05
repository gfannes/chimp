## Portfolio

Amp metadata: `&?portfolio`

A Portfolio is a collection of Programs, or Projects managed together because they compete for the same budget, people, strategy, or organizational attention.

## Program

Amp metadata: `&?program`

- A Program is a coordinated group of related Projects that together deliver a larger business outcome. The Projects may be independently managed, but they are linked by shared goals, dependencies, or benefits.
- Not bounded by a single delivery deadline

Example: Ampersand Metadata Protocol Program

## Project

Amp metadata: `&?project`

- A Project is a temporary effort with a defined goal, scope, owner, and expected outcome. It usually has a beginning and an end.
- Most often the reporting level towards management
- A Project answers:
	- What are we trying to accomplish?
	- Why does it matter?
	- When is it done?
	- Who owns it?

Example: Markdown-based project management CLI tool

## Epic

Amp metadata: `&?epic`

- An Epic is a large body of work that delivers a meaningful feature or capability but is too large to complete as a single story. It usually needs to be broken down into stories.
- An Epic bundles multiple related Stories that together deliver a complete larger capability, feature, or outcome.
- An Epic should describe a coherent capability, not just a bucket of unrelated tasks; isolated Stories can be placed directly under a Project.
- Most often the reporting level towards the development team
- The Epic is where scope flexibility often lives. Some Stories may be essential, while others are useful but skippable. So an Epic can contain MoSCoW scope:
	- must-have stories
	- should-have stories
	- could-have stories
	- deferred stories

Example: Define and Track Work Items in Markdown

## Story

Amp metadata: `&?story`

- A Story is the smallest independently valuable increment of functionality. A story should be small enough to estimate, discuss, implement, and validate.
- Often expressed as a UserStory to stress the value it should deliver
- A Story should usually have:
	- clear user/stakeholder value
	- acceptance criteria
	- small implementation scope
	- a meaningful done/not-done state

Example: As a project maintainer, I want to assign priority to stories so that I can decide what to work on first.

## Task

Amp metadata: `&?task`

- A task is a concrete piece of work needed to complete a story, requirement, or other work item. Tasks are usually implementation-focused and assigned to individuals.
- Tasks describe work to be done, not user value by themselves.

Example:
- Add priority field to Markdown frontmatter parser.
- Update dependency graph schema.
- Write tests for inherited priority behavior.

## Extra levels

Follow extra levels might be useful someday:
- Portfolio: bundles different Programs
- Feature: sits in-between Epic and Story, `&?feature`
- Requirement: not clear, is a bit different, `&?requirement`
- Subtask, `&?subtask`

priority = how important is it?
order = where does it sit in a sequence?
dependency = what must be done first?
scope = is it required for this delivery?
