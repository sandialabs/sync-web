;; Imported from upstream s7test.scm line 15932.
;; Original form:
;; (test (let ((hi 3))
;; 	(let ((e (curlet)))
;; 	  (set! hi (curlet))
;; 	  (object->string e)))
;;       "#1=(inlet 'hi #2=(inlet 'e #1#))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((hi 3))
	(let ((e (curlet)))
	  (set! hi (curlet))
	  (object->string e))))))
       (expected (upstream-safe (lambda () "#1=(inlet 'hi #2=(inlet 'e #1#))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15932 actual expected ok?))
