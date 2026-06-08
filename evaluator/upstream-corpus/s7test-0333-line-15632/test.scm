;; Imported from upstream s7test.scm line 15632.
;; Original form:
;; (test (let ((lst (list 1 2 3)))
;; 	(set! (cdr (cddr lst)) lst)
;; 	(map (lambda (a b)
;; 	       (+ a b))
;; 	     (vector 4 5 6 7 8 9 10)
;; 	     lst))
;;       '(5 7 9 8))

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst (list 1 2 3)))
	(set! (cdr (cddr lst)) lst)
	(map (lambda (a b)
	       (+ a b))
	     (vector 4 5 6 7 8 9 10)
	     lst)))))
       (expected (upstream-safe (lambda () '(5 7 9 8))))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15632 actual expected ok?))
