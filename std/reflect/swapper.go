// Copyright 2016 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

package reflect

// Swapper returns a function that swaps the elements in the provided
// slice.
//
// Swapper panics if the provided interface is not a slice.
func Swapper(slice interface{}) func(i, j int) {
	return func(i, j int) {
		native.swap(slice, i, j)
	}
}
