(define-macro (define-handler name role field)
  `(define (,name request state)
     (let ((user (car request))
           (path (cadr request)))
       (if (or (eq? user ',role) (eq? user 'admin))
           (begin
             (set! (state path) (+ (or (state path) 0) ,field))
             (list 'ok ',name path (state path)))
           (list 'denied ',name user path)))))

(define-handler handle-owner owner 1)
(define-handler handle-writer writer 2)
(define-handler handle-reader reader 0)

(define (dispatch n req st)
  (case n
    ((0) (handle-owner req st))
    ((1) (handle-writer req st))
    (else (handle-reader req st))))

(let ((state (hash-table 'x 1 'y 2 'z 3)))
  (let loop ((i 0) (acc 0) (last '()))
    (if (= i 7500)
        (list acc last (state 'w) (state 'x) (state 'y) (state 'z))
        (let* ((user (case (modulo i 5) ((0) 'admin) ((1) 'owner) ((2) 'writer) ((3) 'reader) (else 'guest)))
               (path (case (modulo i 4) ((0) 'x) ((1) 'y) ((2) 'z) (else 'w)))
               (res (dispatch (modulo i 3) (list user path) state)))
          (loop (+ i 1) (+ acc (length res)) res)))))
