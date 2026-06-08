;; Imported from upstream s7test.scm line 15918.
;; Original form:
;; (test (let ((v1 (make-vector 16 0))
;; 	    (v2 (make-vector 16 0)))
;; 	(set! (v2 12) v2)
;; 	(set! (v1 12) v1)
;; 	(equal? v1 v2))        ; hmmm -- not sure this is correct
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((v1 (make-vector 16 0))
	    (v2 (make-vector 16 0)))
	(set! (v2 12) v2)
	(set! (v1 12) v1)
	(equal? v1 v2)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15918 actual expected ok?))
