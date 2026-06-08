(let* ((rows (let loop ((i 0) (acc '()))
               (if (= i 3000)
                   acc
                   (loop (+ i 1) (cons (list i (+ i 1) (+ i 2)) acc)))))
       (s (format #f "~{.~{+~A+~}.~}" rows)))
  (list (string-ref s 0) (string-ref s 10) (length s)))
