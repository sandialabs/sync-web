;; Imported from upstream s7test.scm line 15150.
;; Original form:
;; (test (let ((h1 (make-hash-table 11))
;; 	    (old-print-length (*s7* 'print-length)))
;; 	(set! (*s7* 'print-length) 32)
;; 	(hash-table-set! h1 "hi" h1)
;; 	(let ((result (object->string h1)))
;; 	  (set! (*s7* 'print-length) old-print-length)
;; 	  (let ((val (string=? result "#1=(hash-table \"hi\" #1#)")))
;; 	    (unless val
;; 	      (format #t ";hash display:~%  ~A~%" result))
;; 	    val)))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((h1 (make-hash-table 11))
	    (old-print-length (*s7* 'print-length)))
	(set! (*s7* 'print-length) 32)
	(hash-table-set! h1 "hi" h1)
	(let ((result (object->string h1)))
	  (set! (*s7* 'print-length) old-print-length)
	  (let ((val (string=? result "#1=(hash-table \"hi\" #1#)")))
	    (unless val
	      (format #t ";hash display:~%  ~A~%" result))
	    val))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 15150 actual expected ok?))
