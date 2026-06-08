(let ((y 10) (m (macro (x) `(+ ,x y)))) (hash-table-ref (hash-table 'a m) 'a 1))
