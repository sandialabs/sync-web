;; Imported from upstream s7test.scm line 15616.
;; Original form:
;; (test (let ((lst (list 1 2 3))
;; 	    (result ()))
;; 	(set! (cdr (cddr lst)) lst)
;; 	(for-each (lambda (a b)
;; 		    (set! result (cons (+ a b) result)))
;; 		  (list 4 5 6)
;; 		  lst)
;; 	result)
;;       '(9 7 5))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1 2 3))
	    (result ()))
	(set! (cdr (cddr lst)) lst)
	(for-each (lambda (a b)
		    (set! result (cons (+ a b) result)))
		  (list 4 5 6)
		  lst)
	result))))
       (expected (upstream-safe (lambda () '(9 7 5))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15616 actual expected ok?))
