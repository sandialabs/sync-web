;; Imported from upstream s7test.scm line 5103.
;; Original form:
;; (test (let ((sym000000000000000000000 3))
;; 	(let ((sym000000000000000000001 4))
;; 	  (+ sym000000000000000000000 sym000000000000000000001)))
;;       7)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((sym000000000000000000000 3))
	(let ((sym000000000000000000001 4))
	  (+ sym000000000000000000000 sym000000000000000000001))))))
       (expected (upstream-safe (lambda () 7)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5103 actual expected ok?))
