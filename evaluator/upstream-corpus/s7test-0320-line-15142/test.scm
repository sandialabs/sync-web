;; Imported from upstream s7test.scm line 15142.
;; Original form:
;; (test (let* ((v1 (vector 1 2)) (v2 (vector v1)))
;; 	(vector-set! v1 1 v1)
;; 	(string=? (object->string v2) "#(#1=#(1 #1#))"))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let* ((v1 (vector 1 2)) (v2 (vector v1)))
	(vector-set! v1 1 v1)
	(string=? (object->string v2) "#(#1=#(1 #1#))")))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15142 actual expected ok?))
