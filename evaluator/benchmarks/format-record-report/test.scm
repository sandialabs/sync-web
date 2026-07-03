(let* ((rows (let loop ((i 0) (acc '()))
               (if (= i 20000)
                   acc
                   (loop (+ i 1) (cons (list i (remainder (* i 7) 101) (+ i 1)) acc)))))
       (s (format #f "~{.~{+~A+~}.~}" rows)))
  (list (string-ref s 0) (string-ref s 200) (length s)))
