(define-macro (make-node id left right)
  `(vector 'node ,id ,left ,right))
(define-macro (node-id n) `(,n 1))
(define-macro (node-left n) `(,n 2))
(define-macro (node-right n) `(,n 3))

(let loop ((i 0) (acc 0))
  (if (= i 40000)
      acc
      (let ((n (make-node i (list i (+ i 1)) (hash-table 'r (+ i 2)))))
        (loop (+ i 1) (+ acc (node-id n) (car (node-left n)) ((node-right n) 'r))))))
