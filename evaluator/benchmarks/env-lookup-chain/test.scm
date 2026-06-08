(let ((a 1))
  (let ((b 2))
    (let ((c 3))
      (let ((d 4))
        (let ((e 5))
          (let loop ((i 0) (acc 0))
            (if (= i 300000)
                acc
                (loop (+ i 1) (+ acc a b c d e)))))))))
