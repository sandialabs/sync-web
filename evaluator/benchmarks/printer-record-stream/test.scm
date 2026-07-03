(let ((p (open-output-string)))
  (let loop ((i 0))
    (if (= i 12000)
        (let ((s (get-output-string p)))
          (list (string-ref s 0) (string-ref s 100) (length s)))
        (begin
          (write (list 'node i (hash-table 'left (remainder i 17) 'right (vector i (+ i 1))) (inlet 'tag 'record 'rev i)) p)
          (newline p)
          (loop (+ i 1))))))
