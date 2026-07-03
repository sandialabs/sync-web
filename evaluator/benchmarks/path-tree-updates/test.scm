(define (put al k v)
  (let ((cell (assoc k al)))
    (if cell
        (begin (set-cdr! cell v) al)
        (cons (cons k v) al))))

(define (get al k)
  (let ((cell (assoc k al)))
    (if cell (cdr cell) #f)))

(define (path-set tree a b c value)
  (let* ((ta (or (get tree a) '()))
         (tb (or (get ta b) '()))
         (tc (put tb c value))
         (ta2 (put ta b tc)))
    (put tree a ta2)))

(define (path-get tree a b c)
  (let ((ta (get tree a)))
    (if ta
        (let ((tb (get ta b)))
          (if tb (get tb c) #f))
        #f)))

(let build ((i 0) (tree '()))
  (if (= i 5000)
      (let read ((j 0) (acc 0))
        (if (= j 5000)
            acc
            (let ((v (path-get tree (remainder j 37) (remainder j 23) (remainder j 19))))
              (read (+ j 1) (+ acc (if v (car v) 0))))))
      (build (+ i 1)
             (path-set tree (remainder i 37) (remainder i 23) (remainder i 19)
                       (list i (* i 3))))))
