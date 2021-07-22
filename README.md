# Goscript
Go specs implemented as a script language.

### The Goal
+ Runs most pure Go code, probably add some dynamic features if requested.

### How do I try
The project "engine" is the entry/wrapper. there are test cases in [here](https://github.com/oxfeeefeee/goscript/tree/master/engine/tests to browse through.
+ Make sure your Rust installation is up to date.
+ Clone this repository.
+ Go to goscript/engine
+ Run cargo test -- --nocapture


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
+ There are five projects: 
    - parser -- a port of the official implementation that comes with the Go installer.
    - type checker  -- a port of the official implementation that comes with the Go installer.
    - codegen -- generates the bytecode.
    - vm -- runs the bytecode.
    - engine -- the wrapper.

### Progress
+ Language: the biggest missing part is goroutine/channel/defer, supports most features, some of them are, probably for the first time, implemented in a script language :), like Pointer/Interface/Struct.
+ Standard library: just got started.
+ Production readiness: far from. The parser and the type checker are probably ok because they were ported and passes
the test cases comes with the original code. The backend has a lot of rough edges, and we need much more test cases.
+ Next step: no new features for now, ponish then work on the standard library.

### Get in touch
+ email: oxfeeefeee at gmail.
+ wechat: oxfeeefeee