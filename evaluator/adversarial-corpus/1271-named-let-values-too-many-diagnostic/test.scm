(let loop ((i 0)) (if (= i 3) i (loop (values (+ i 1) 99))))
