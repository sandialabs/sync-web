;; Imported from upstream s7test.scm line 5107.
;; Original form:
;; (test (let ((11-1 10)
;; 	    (2012-4-19 21)
;; 	    (1+the-road 18)
;; 	    (-1+2 1)
;; 	    (1e. 2)
;; 	    (0+i' 3)
;; 	    (0.. 4))
;; 	(+ 11-1 2012-4-19 1+the-road -1+2 1e. 0+i' 0..))
;;       59)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((11-1 10)
	    (2012-4-19 21)
	    (1+the-road 18)
	    (-1+2 1)
	    (1e. 2)
	    (0+i' 3)
	    (0.. 4))
	(+ 11-1 2012-4-19 1+the-road -1+2 1e. 0+i' 0..)))))
       (expected (upstream-safe (lambda () 59)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5107 actual expected ok?))
