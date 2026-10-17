# Working style

## Ask before deciding
- Before deciding or changing anything (specs, design, API names,
  constants, file layout, dependencies), ask the user first.
  Do not go ahead on your own.
- Even for exploratory work (trying something out, running an
  experiment), say what you are about to do before doing it.
- When asking, lay out the options with the pros and cons of each,
  and give a recommendation with the reason.
- After the work, list what changed and why, so every change can be followed.

## Explain concretely
- Do not stop at a short summary. Explain what happens and how it
  works, step by step.
- Include concrete examples: code, numbers, inputs and outputs, commands.
- Define technical terms the first time they appear.

## Role
- The user owns the project and makes the decisions. Claude is an
  assistant that helps; it does not take over.

## Branches
- New features: `feature/<name>`
- Fixes: `fix/<name>`

## Repository and licensing
- Treat the repository as a work in progress unless the user explicitly says otherwise.
- Do not make licensing, attribution, copyright, or repository publication changes without asking the user first.
- When working on a fork, preserve existing upstream copyright notices, license headers, and attribution notices.
- Before distributing or preparing a release of a fork, audit the applicable license requirements and report any unresolved issues.
- Do not assume that a public repository is ready for release.
- Distinguish between changes to source code, generated artifacts, vendored dependencies, and repository configuration when reviewing Git diffs.