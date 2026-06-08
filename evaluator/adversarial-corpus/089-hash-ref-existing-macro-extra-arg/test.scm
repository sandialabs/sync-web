(let ((m (macro (x) `(+ ,x 1)))) (hash-table-ref (hash-table 'a m) 'a 2))
