;; Collatz sequence generation and range analysis.

(define (collatz-next n)
  (if (even? n)
      (/ n 2)
      (+ (* 3 n) 1)))

(define (collatz-sequence n)
  (let loop ((x n) (out '()))
    (if (= x 1)
        (reverse (cons 1 out))
        (loop (collatz-next x) (cons x out)))))

(define (collatz-length n)
  (let loop ((x n) (steps 1))
    (if (= x 1)
        steps
        (loop (collatz-next x) (+ steps 1)))))

(define (collatz-peak n)
  (let loop ((x n) (peak n))
    (if (= x 1)
        peak
        (let ((next (collatz-next x)))
          (loop next (max peak next))))))

(define (collatz-records limit)
  (let loop ((n 1) (best-n 1) (best-len 1) (records '()))
    (if (> n limit)
        (reverse records)
        (let ((len (collatz-length n)))
          (if (> len best-len)
              (loop (+ n 1) n len (cons (list n len (collatz-peak n)) records))
              (loop (+ n 1) best-n best-len records))))))

(define (take xs n)
  (if (or (= n 0) (null? xs))
      '()
      (cons (car xs) (take (cdr xs) (- n 1)))))

(let* ((seq-27 (collatz-sequence 27))
       (seq-97 (collatz-sequence 97))
       (records (collatz-records 100)))
  (list
    (list 'seq-13 (collatz-sequence 13))
    (list 'len-27 (length seq-27))
    (list 'first-20-of-27 (take seq-27 20))
    (list 'peak-27 (collatz-peak 27))
    (list 'len-97 (length seq-97))
    (list 'records-to-100 records)
    (list 'best-to-100 (car (reverse records)))))
