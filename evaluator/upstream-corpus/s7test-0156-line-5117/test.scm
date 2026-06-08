;; Imported from upstream s7test.scm line 5117.
;; Original form:
;; (test (let ((name "hiho"))
;; 	(string-set! name 2 #\null)
;; 	(symbol? (string->symbol name)))
;;       #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let ((name "hiho"))
	(string-set! name 2 #\null)
	(symbol? (string->symbol name))))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 5117 actual expected ok?))
