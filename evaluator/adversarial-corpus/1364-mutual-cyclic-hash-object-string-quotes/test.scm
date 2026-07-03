(let ((x (hash-table)) (y (hash-table))) (set! (x 'a) y) (set! (y 'b) x) (object->string x))
