;; Imported from upstream s7test.scm line 15146.
;; Original form:
;; (test (let ((v1 (make-vector 3 1)))
;; 	(vector-set! v1 0 (cons 3 v1))
;; 	(string=? (object->string v1) "#1=#((3 . #1#) 1 1)"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((v1 (make-vector 3 1)))
	(vector-set! v1 0 (cons 3 v1))
	(string=? (object->string v1) "#1=#((3 . #1#) 1 1)")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15146 actual expected ok?))
