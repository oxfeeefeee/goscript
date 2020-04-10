# Goscript
A compiler and VM in Rust that runs Go.

### The Goal
+ To be full compatible with Go, i.e. to be able to run any valid Go code.

### Use Cases
+ As an embedded language like Lua.
+ As a glue language like Python.

### Rationale
+ A scripting language that is Rust friendly is needed.
+ Go is popular and easy(even as a scripting language).
+ If go were an embedded language it would be way better than Lua.
+ If Go were a glue language it would be way better than Python, in terms of project maintainability.

### Implementation
+ The parser is a port of the official implementation shipped with the Go installer.
+ The VM is based on that of Lua/Wren.

### Progress
+ The parser is basically finished, we still need to port the Type Checker.
+ A piece of code can be run on the VM as a proof of concept, there is a lot to be done.
+ In short, this is just the beginning, it's impossible to do it all by myself. 

### Join us
+ If you like the idea, and would like to help, please contact oxfeeefeee at gmail.