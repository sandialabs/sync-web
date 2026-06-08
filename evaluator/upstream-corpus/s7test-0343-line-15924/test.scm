;; Imported from upstream s7test.scm line 15924.
;; Original form:
;; (test (let ((lst1 (list 1))
;; 	    (lst2 (list 1)))
;; 	(set-cdr! lst1 lst1)
;; 	(set-cdr! lst2 lst2)
;; 	(equal? lst1 lst2))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((lst1 (list 1))
	    (lst2 (list 1)))
	(set-cdr! lst1 lst1)
	(set-cdr! lst2 lst2)
	(equal? lst1 lst2)))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15924 actual expected ok?))
