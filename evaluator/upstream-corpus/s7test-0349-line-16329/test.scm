;; Imported from upstream s7test.scm line 16329.
;; Original form:
;; (test (catch #t
;; 	   (lambda ()
;; 	     (hooked-catch a-hook (abs "hi")))
;; 	   (lambda args
;; 	     123))
;; 	 123)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (catch #t
	   (lambda ()
	     (hooked-catch a-hook (abs "hi")))
	   (lambda args
	     123)))))
       (expected (upstream-safe (lambda () 123)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 16329 actual expected ok?))
