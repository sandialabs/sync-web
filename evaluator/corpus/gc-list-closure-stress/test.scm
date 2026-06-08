(define (make-adders n)
  (let loop ((i 0) (out '()))
    (if (= i n)
        out
        (loop (+ i 1) (cons (let ((base i)) (lambda (x) (+ base x))) out)))))

(define (sum-adders fs)
  (let loop ((rest fs) (i 0) (sum 0))
    (if (null? rest)
        sum
        (loop (cdr rest) (+ i 1) (+ sum ((car rest) i))))))

(let* ((fs (make-adders 300))
       (nested (let loop ((i 0) (out '()))
                 (if (= i 400)
                     out
                     (loop (+ i 1) (cons (list i (* i i) (modulo i 7)) out))))))
  (list (length fs) (sum-adders fs) (length nested) (car nested) (car (reverse nested))))
