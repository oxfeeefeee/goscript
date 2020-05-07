# Goscript
Go specs implemented as a scripting language in Rust.

### The Goal
+ To be full compatible with Go, i.e. to be able to run any valid Go code.(But only a subset of functionalities will be supported in version 0.1)

### Use Cases
+ As an embedded language like Lua.
+ As a glue language like Python.

### Rationale
+ A scripting language that is Rust friendly is needed.
+ Go is popular and easy(even as a scripting language).
+ If Go were an embedded language it would be way better than Lua.
+ If Go were a glue language it would be way better than Python, in terms of project maintainability.

### Implementation
+ The parser is a port of the official implementation shipped with the Go installer.
+ The VM is based on that of Lua/Wren.

### Progress
+ The parser is basically finished, we still need to port the Type Checker.
+ You can take a look at [here](https://github.com/oxfeeefeee/goscript/tree/master/backend/tests/data) to see what it can run for now.
+ In short, this is just the beginning, it's impossible to do it all by myself. 

### Join us
+ If you like the idea, and would like to help, please contact oxfeeefeee at gmail.