(let* ((a (vector 1)) (b (hash-table)) (c (list 1))) (vector-set! a 0 b) (set! (b 'c) c) (set-cdr! c a) (object->string a))
