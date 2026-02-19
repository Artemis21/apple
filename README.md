# Apple

A very minimal functional compiler, written in Rust with an LLVM backend.

Example:

```
(block
    (fn curry_add ((x _)) _ (block
        (fn inner ((y _)) _
            (+ x y)
        )
        inner
    ))

    (fn print_mapped_array ((arr _) (f _)) _
        (for i arr
            (print (f i))
        )
    )

    (let (factor limit) _ (, 2 10.0))

    (fn weird ((x _)) _ (block
        (let y _ (to_real (* x factor)))
        (if (< y limit)
            ((curry_add y) 1.0)
            (normal 0.0 1.0)
        )
    ))

    (print_mapped_array (.. 1 10) weird)
)
```

Output:

```
3.000000
5.000000
7.000000
9.000000
336736864456782105775439872.000000
0.000000
13915880780094861475840.000000
211660013157571985670144.000000
112098729821300864932370706333696.000000
```

## Setting up LLVM (Fedora)

```
sudo dnf install llvm-devel
LLVM_SYS_211_PREFIX=/usr cargo run
```
