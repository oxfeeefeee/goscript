# Goscript  

An alternative implementation of Golang specs, written in Rust for embedding or wrapping.

[中文](#Goscript_Readme_中文版)

## Website

<https://goscript.dev/>

## What

+ In other words, it's just another flavor of Go.
+ It's designed to work with other Rust code in the case where you want to offer the users of your library/app the ability to use a "simpler" language to call the native (Rust) code, like what Lua is for Redis/WoW, or Python for NumPy/Sublime Text.

## Why

+ It's native to Rust, and your Rust project is looking for a scripting language.
+ Go is nice and easy and you are familiar with it.
+ To Rust programmers, it should be very well understood that compiler helps, as a strictly typed language, Goscript's type checker helps a lot compared to dynamically type languages, especially when your codebase gets larger.

## How

[Goscript Internals I: Overview](https://goscript.dev/posts/goscript_internals_I_overview_en)

## When

+ This readme update serves as an alpha release announcement.
+ Almost all the language features are implemented as per the specs(pre-1.18, i.e. excluding generics).
+ Part of the standard library is ported, which is not fully working.
+ There are still a lot of work to be done besides libraries, like API cleanup, Documentation, and a lot more test cases.
+ Overall it's considered buggy although it passes dozens of test [cases](https://github.com/oxfeeefeee/goscript/tree/master/engine/tests).
+ A beta release is expected in about a year from now.

## Who

+ email: [`pb01005051` at Gmail](mailto:pb01005051@gmail).
+ discord: [join](https://discord.gg/tYwqXEhVqa)

## FAQ

+ Is this a new language?

  No, this is Go by definition. You can think of it as a hybrid of Go and Python, it takes the design of Go and implements it the way Python does. So, I think it's also fair to call it a scripting language.

+ Why not a new language?

  It's much easier to implement a language than to design one, I don't think I'm able to design a language that people would invest their time to learn. Go may be not perfect as a scripting language, but to me it works Ok.

+ Why not just use Python/Lua (with Rust)?

  People will/do, and Goscript can be a reasonable alternative, a strong type system can be a friend not enemy of your creativity.

+ Is Goscript fast/slow?

  I'm afraid it may be slower than you think. It's a VM based implementation without JIT, so there is an upper bound. Also, there is a tradeoff between performance and project complexity, we don't want a embedding language to be too big. For now, you can only expect a performance comparable to Python, I'm not sure how far we'll go with optimization in the long term.

+ Does Goscript support multi-threading/parallelization?

  No, that'd hurt single-thread performance and make things a lot harder to implement. I guess that's also why Python/Lua doesn't support parallelization. But it supports concurrency exceptionally well via goroutine :). It's also possible to introduce parallelization with libraries in the future.

+ When can we actually use Goscirpt?

  As mentioned above, I hope we can get it production ready in a year. I started this project three years ago, and I'll do my best not to abandon it.

+ How can I help?

  + Report issues.
  + Send a PR if it's a minor fix.
  + If you plan to contribute complex changes, please contact me for a discussion first.

## Just want to see if it does run Go code

+ Make sure your Rust installation is up to date.
+ Clone this repository.
+ Go to goscript/engine
+ Put whatever you want in [temp.gos](https://github.com/oxfeeefeee/goscript/tree/master/engine/tests/std/temp.gos)
+ Run `cargo test temp -- --nocapture`
+ Or it should be trivial to make an executable that runs Go code following [test.rs](https://github.com/oxfeeefeee/goscript/tree/master/engine/tests/test.rs)
+ Your code doesn't run? sorry, you can take a look at what do run in the test folder.

-----------------

# Goscript_Readme_中文版

Go语言规范非的官方实现，用于Rust项目的内嵌或封装。

## 网站

<https://goscript.dev/>

## 啥？

+ 简单地说，它是另一个风味的Go语言。
+ 其作用是用于其他的Rust项目，有时候你需要用一个更简单的语言封装和调用底层的Rust代码。就像Lua之于Redis/WoW，或者Python之于NumPy/Sublime Text。

## 为？

+ 当你的Rust项目需要一个用Rust写的脚本语言。
+ Go语言简单易学且已经被广泛使用。
+ Rust程序员应该已经体会到编译器是编程助手，作为一个严格类型的语言，Goscript相对于其他动态语言更能帮你写好程序，特别是代码量变大的时候。

## 咋？

[Goscript 设计原理一: 总览](https://goscript.dev/posts/goscript_internals_I_overview_zh)

## 时？

+ 本次readme更新可以看作是alpha版本发布。
+ 几乎所有的语言特性都实现了(pre-1.18版本，即不包含范型)。
+ 移植了部分官方库，且这部分也没完全好。
+ 还有很多其他工作要做，比如API整理，文档，大量完备的测试用例等。
+ 总体上bug可能还不少，但是已经能通过很多[测试用例](https://github.com/oxfeeefeee/goscript/tree/master/engine/tests)。
+ 预期在一年以内发布beta版本。

## 谁？

+ email: [`pb01005051` at Gmail](mailto:pb01005051@gmail).
+ discord: [加入](https://discord.gg/tYwqXEhVqa)

## 问

+ 这是个新语言吗?

  不是，因为它实现了Go语言的规范，所以理论上就是Go语言（的一种）。它可以看作是一个Go和Python的混血：用Python语言的实现方式实现了Go语言。所以你叫它脚本语言也对。

+ 为什么不弄个新语言?

  设计一门语言比实现一门语言难多了，自认为没有能力设计一门语言好到能让其他程序员愿意学习它的新的“更好的”语法。Go作为脚本语言可能不完美，但是我觉得够用。

+ 为什么不直接用 Python/Lua (在Rust项目中)?

  大家会用Python/Lua的, Goscirpt只是提供了另外一个选项，强类型系统可以帮你写好代码而不是限制你。

+ Goscript快吗/慢吗?

  可能比你想象的要慢。它是没有JIT的基于虚拟机运行的，性能上存在上限，而且我们需要在性能和复杂度上找平衡，不希望它大到不适合被嵌入。目前我们只能期望一个跟Python相当的性能，远期优化能做到什么地步目前不好预测。

+ Goscirpt支持多线程/并行吗?

  不支持，因为多线程支持会降低单线程的性能，并且写起来也复杂很多。我猜这也是Python/Lua不支持并行的原因。不过对并发的支持是无与伦比的好:)，因为有goroutine。不排除将来用库的方式支持并行。

+ 什么时候能真正用?

  如前面提到的，我希望能在一年内达到生产可用的成熟度。这个项目是三年前开始的，我会尽可能完成它。

+ 可以怎么参与项目？

  + 提issue。
  + 小的改动可以直接提PR。
  + 大的改动请先联系我讨论一下。

## 就想跑点Go代码试试

+ 安装最新版的Rust。
+ Clone本项目。
+ 到goscript/engine目录。
+ 在[temp.gos](https://github.com/oxfeeefeee/goscript/tree/master/engine/tests/std/temp.gos) 写你想写的Go代码。
+ 运行 `cargo test temp -- --nocapture`
+ 或者参照[test.rs](https://github.com/oxfeeefeee/goscript/tree/master/engine/tests/test.rs) 弄个可以执行Go代码的exe。
+ 你的代码跑不了？不好意思，不过你可以看看测试文件夹里那些可以跑的代码。
