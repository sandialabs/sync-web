;; Imported from upstream s7test.scm line 15105.
;; Original form:
;; (test (let ((v (vector 1 2)))
;; 	(vector-set! v 0 v)
;; 	(string=? (object->string v) "#1=#(#1# 2)"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((v (vector 1 2)))
	(vector-set! v 0 v)
	(string=? (object->string v) "#1=#(#1# 2)")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15105 actual expected ok?))
