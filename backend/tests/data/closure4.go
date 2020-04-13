package main

func main() {
    a := 44
    b := func() func() int {
        c := 3
        return func()int {
            d := 2
			return a + 1 + c + d
        }
    }
    e := func() int {
        c := b()() + 10
        return c + a
    }
    f := e()
}

//104