(let ((h (hash-table 'a 1 'b 2)))
  (hash-table-set! h 'c 3)
  (list
    (list 'ref-a (hash-table-ref h 'a))
    (list 'apply-b (h 'b))
    (list 'set-c (h 'c))
    (list 'entries (hash-table-entries h))))
