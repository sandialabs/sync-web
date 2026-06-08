(let ((m (macro args `(list ,@args)))) (hash-table-ref (hash-table 'a m) 'a 1 2 3))
