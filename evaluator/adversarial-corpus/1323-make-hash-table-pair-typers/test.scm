(let ((h (make-hash-table 8 #f (cons symbol? integer?)))) (hash-table-set! h 'a 1) h)
