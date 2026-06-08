(let ((m (macro (x y) `(+ ,x ,y)))) (hash-table-ref (hash-table 'a m) 'a 2 3))
