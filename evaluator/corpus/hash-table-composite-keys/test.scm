(let ((h (make-hash-table 17 equal?)))
  (hash-table-set! h '(a b) 'list-key)
  (hash-table-set! h #(1 2) 'vector-key)
  (hash-table-set! h "xy" 'string-key)
  (list
    (hash-table-ref h '(a b))
    (hash-table-ref h #(1 2))
    (hash-table-ref h "xy")
    (hash-table-entries h)))
