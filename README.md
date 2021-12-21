# Goscript
A script language like Python or Lua written in Rust, with exactly the same syntax as Go's.

### The Goal
+ Runs most pure Go code, probably add some dynamic features if requested.

### How do I try
The project "engine" is the entry/wrapper. there are test cases in [here](https://github.com/oxfeeefeee/goscript/tree/master/engine/tests) to browse through.
+ Make sure your Rust installation is up to date.
+ Clone this repository.
+ Go to goscript/engine
+ Run `cargo test -- --nocapture`


### Use Cases
+ As an embedded language like Lua.
+ As a glue language like Python.

### Rationale
+ A scripting language that can be embedded in Rust.
+ Go is popular and easy(even as a scripting language), why invent new grammars?
+ In some cases, when your project gets big:
    - If Go were an embedded language it would be better than Lua.
    - If Go were a glue language it would be better than Python, in terms of project maintainability.
+ I found a new hammer(Rust) that I like, and decided to use it on a nail(Go) that I like. 

### Implementation
+ There are five projects: 
    - parser -- a port of the official implementation that comes with the Go installer.
    - type checker  -- a port of the official implementation that comes with the Go installer.
    - codegen -- generates the bytecode.
    - vm -- runs the bytecode.
    - engine -- the wrapper.

### Progress
+ Language: Feature complete (except the features that are so insignificant that I forgot -_-) Some of the features are, probably for the first time, implemented in a script language, like Select/Defer/Pointer/Interface/Struct.
+ Standard library: just got started.
+ Production readiness: far from. The parser and the type checker are probably ok because they were ported and passes
the test cases comes with the original code. The backend has a lot of rough edges, and we need much more test cases.
+ Next step: no new features for now, polish then work on the standard library.

### Get in touch
+ email: [`pb01005051` at Gmail](mailto:pb01005051@gmail).