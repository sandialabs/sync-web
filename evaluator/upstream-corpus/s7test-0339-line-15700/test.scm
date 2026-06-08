;; Imported from upstream s7test.scm line 15700.
;; Original form:
;; (test (let ((f #f))
;; 	(set! f (lambda ()
;; 		  (let* ((code (procedure-source f))
;; 			 (pos (- (length code) 1)))
;; 		    (set! (code pos) (+ (code pos) 1)))
;; 		  1))
;; 	(f) (f) (f))
;;       4)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((f #f))
	(set! f (lambda ()
		  (let* ((code (procedure-source f))
			 (pos (- (length code) 1)))
		    (set! (code pos) (+ (code pos) 1)))
		  1))
	(f) (f) (f)))))
       (expected (upstream-safe (lambda () 4)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15700 actual expected ok?))
