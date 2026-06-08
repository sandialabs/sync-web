;; Imported from upstream s7test.scm line 15648.
;; Original form:
;; (test (let ((lst (list 1 2 3))
;; 	    (ctr 0))
;; 	(set! (cdr (cddr lst)) lst)
;; 	(for-each (lambda (a b)
;; 		    (set! ctr (+ ctr (+ a b))))
;; 		  lst ())
;; 	ctr)
;;       0)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1 2 3))
	    (ctr 0))
	(set! (cdr (cddr lst)) lst)
	(for-each (lambda (a b)
		    (set! ctr (+ ctr (+ a b))))
		  lst ())
	ctr))))
       (expected (upstream-safe (lambda () 0)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15648 actual expected ok?))
