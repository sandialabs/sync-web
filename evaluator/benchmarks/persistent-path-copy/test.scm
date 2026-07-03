(define (make-node) (vector #f #f #f #f))

(define (node-ref n k) (n k))
(define (node-set n k v)
  (let ((m (copy n)))
    (set! (m k) v)
    m))

(define (tree-set root a b c value)
  (let* ((na (or (node-ref root a) (make-node)))
         (nb (or (node-ref na b) (make-node)))
         (nb2 (node-set nb c value))
         (na2 (node-set na b nb2)))
    (node-set root a na2)))

(define (tree-get root a b c)
  (let ((na (node-ref root a)))
    (if na
        (let ((nb (node-ref na b)))
          (if nb (node-ref nb c) #f))
        #f)))

(let loop ((i 0) (root (make-node)))
  (if (= i 3000)
      (let read ((j 0) (acc 0))
        (if (= j 3000)
            acc
            (let ((v (tree-get root (remainder j 4) (remainder (+ j 1) 4) (remainder (+ j 2) 4))))
              (read (+ j 1) (+ acc (if v v 0))))))
      (loop (+ i 1)
            (tree-set root (remainder i 4) (remainder (+ i 1) 4) (remainder (+ i 2) 4) i))))
