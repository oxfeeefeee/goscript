# Goscript
Go specs implemented as a scripting language in Rust.

### The Goal
+ To be full compatible with Go, i.e. to be able to run any valid Go code.(But only a subset of features will be supported in version 0.1)

### Use Cases
+ As an embedded language like Lua.
+ As a glue language like Python.

### Rationale
+ A scripting language that is Rust friendly is needed.
+ Go is popular and easy(even as a scripting language).
+ In some cases, when your project gets big:
    - If Go were an embedded language it would be better than Lua.
    - If Go were a glue language it would be better than Python, in terms of project maintainability.

### Implementation
+ The parser is a port of the official Go implementation.
+ The VM is based on that of Lua/Wren.

### Progress/Roadmap
+ You can take a look at [here](https://github.com/oxfeeefeee/goscript/blob/master/codegen/tests/data/leetcode5.gos) to see what it can run for now.
+ The parser works fine, the core part of the VM is basically working.
+ 4 major components to go: TypeChecker, GC, Library, API. I'm planning to work on the TypeCheker in the next few weeks before go back to the VM.

### Join us
+ If you like the idea, and would like to help, please contact oxfeeefeee at gmail.