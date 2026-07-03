(let loop ((i 0))
  (if (= i 1)
      (or (values #f #t) 'no)
      (loop 1)))
