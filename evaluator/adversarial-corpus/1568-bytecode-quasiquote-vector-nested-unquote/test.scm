(let loop ((i 0))
  (if (= i 1)
      `#((unquote i))
      (loop 1)))
