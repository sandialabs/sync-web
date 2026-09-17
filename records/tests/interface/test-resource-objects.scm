(lambda (make-interface-harness)
  (with-let (make-interface-harness :journals 2 :users '(alice bob))
    (define (error-result? result)
      (and (list? result) (pair? result) (eq? (car result) 'error)))

    (define counter-class
      '(define-class (counter)
         (define-method (*init* self value)
           (set! (self '(1 0)) (expression->byte-vector value))
           (set! (self '(1 1)) (expression->byte-vector 100))
           value)

         (define-method (value self)
           (byte-vector->expression (self '(1 0))))

         (define-method (hidden self)
           (byte-vector->expression (self '(1 1))))

         (define-method (increment! self amount)
           (let ((value (+ (byte-vector->expression (self '(1 0))) amount)))
             (set! (self '(1 0)) (expression->byte-vector value))
             value))

         (define-method (mutate-error! self)
           (set! (self '(1 0)) (expression->byte-vector 99))
           (error 'expected "resource method failed"))

         (define-method (bytes self)
           (expression->byte-vector '(direct bytes)))

         (define-method (mutate-raw! self)
           (set! (self '(1 0)) (expression->byte-vector 99))
           (self))

         (define-method (mutate-vector! self)
           (set! (self '(1 0)) (expression->byte-vector 99))
           (vector 1 2))

         (define-method (mutate-procedure! self)
           (set! (self '(1 0)) (expression->byte-vector 99))
           (lambda (value) value))))

    (define changed-class
      '(define-class (counter-v2)
         (define-method (value self) 2)))

    (define path '(*state* alice counter))
    (define copy-path '(*state* alice counter-copy))
    (define create-only-path '(*state* alice create-only-counter))

    ;; Object creation can atomically require exact target absence.
    (test-submit
     ((alice journal-1 'put!)
      `((path ,create-only-path) (value ,counter-class) (object? #t)
        (expected (nothing)) (expression? #t)))
     :expect #t)
    (test-submit
     ((alice journal-1 'put!)
      `((path ,create-only-path) (value ,counter-class) (object? #t)
        (expected (nothing)) (expression? #t)))
     :expect #f)

    ;; put! stores one uninitialized shell without invoking *init*.
    (test-submit
     ((alice journal-1 'put!)
      `((path ,path) (value ,counter-class) (object? #t) (expression? #t)))
     :expect #t)
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (expression? #t)))
     :expect (lambda (result)
               (and (equal? (cadr (assoc 'class result)) 'counter)
                    (byte-vector? (cadr (assoc 'object-hash result)))
                    (byte-vector? (cadr (assoc 'code-hash result))))))
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method *api*) (arguments ()) (read-only? #t)
        (expression? #t)))
     :expect '(*name* *api* *class* *init* value hidden increment! mutate-error!
               bytes mutate-raw! mutate-vector! mutate-procedure!))
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method *init*) (arguments (5)) (expression? #t)))
     :expect 5)
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method value) (arguments ()) (expression? #t)))
     :expect 5)

    ;; Duplicate batch paths observe prior staged successors in request order.
    (test-submit
     ((alice journal-1 'use-batch!)
      :paths `(,path ,path ,path)
      :methods '(increment! increment! value)
      :arguments '((1) (2) ())
      :expression? #t)
     :expect '(6 8 8))
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method value) (arguments ()) (expression? #t)))
     :expect 8)
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method bytes) (arguments ()) (expression? #f)))
     :expect (lambda (result)
               (and (byte-vector? result)
                    (equal? (byte-vector->expression result) '(direct bytes)))))

    ;; Method errors and unsupported runtime results leave staged state unchanged.
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method mutate-error!) (arguments ()) (expression? #t)))
     :expect error-result?)
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method mutate-raw!) (arguments ()) (expression? #t)))
     :expect error-result?)
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method mutate-vector!) (arguments ()) (expression? #t)))
     :expect error-result?)
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method mutate-procedure!) (arguments ()) (expression? #t)))
     :expect error-result?)
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method value) (arguments ()) (expression? #t)))
     :expect 8)

    ;; Read-only use returns the same validated result but discards successors
    ;; and leaves both staged state and transition history unchanged.
    (test-submit
     (raw journal-1
          '(*call* "pass-1"
                   (lambda (root)
                     (let* ((ledger
                             (sync-eval
                              ((root 'get) '(root object ledger))))
                            (stage (sync-digest ((ledger '~field!) 'stage)))
                            (transition ((ledger '~get) '(*transition*)))
                            (result ((ledger 'use!)
                                     '(*state* alice counter)
                                     'increment! '(10) #t)))
                       (list result
                             (equal? stage
                                     (sync-digest ((ledger '~field!) 'stage)))
                             (equal? transition
                                     ((ledger '~get) '(*transition*))))))))
     :expect '(18 #t #t))
    (test-submit
     ((alice journal-1 'use-batch!)
      :paths `(,path ,path)
      :methods '(increment! value)
      :arguments '((10) ())
      :read-only? #t
      :expression? #t)
     :expect '(18 8))
    (test-submit
     ((alice journal-1 'use!)
      `((path ,path) (method value) (arguments ())
        (read-only? #t) (expression? #t)))
     :expect 8)
    (test-submit
     ((alice journal-1 'use-batch!)
      :paths `(,path) :methods '(value) :arguments '(() )
      :read-only? '(#t) :expression? #t)
     :expect error-result?)

    ;; copy! preserves the exact object node and use! follows the copied occupant.
    (test-submit
     ((alice journal-1 'copy!) `((source ,path) (path ,copy-path)))
     :expect #t)
    (test-submit
     ((alice journal-1 'use!)
      `((path ,copy-path) (method value) (arguments ()) (expression? #t)))
     :expect 8)

    (test-submit ((*journal* journal-1 'step!)) :expect 1)

    ;; Active scalar and batch proofs retain only method-selected state while
    ;; preserving the exact committed root and local recalculation result.
    (test-submit
     (raw journal-1
          '(*call* "pass-1"
                   (lambda (root)
                     (let* ((standard
                             (sync-eval ((root 'get) '(root object standard))))
                            (ledger
                             (sync-eval ((root 'get) '(root object ledger))))
                            (head ((ledger '~head)
                                   '(0 (*state* alice counter))))
                            (scalar
                             ((standard 'deserialize)
                              ((ledger '~trace) 0
                               '(*state* alice counter) #f 'value '() #t)))
                            (blank
                             ((standard 'deserialize)
                              ((ledger '~trace) 0
                               '(*state* alice counter) #f #f '() #t)))
                            (batch-response
                             ((ledger 'retrieve-batch)
                              '((0 *state* alice counter)
                                (0 *state* alice counter))
                              #f #f #f #t '(value value) '(() ())))
                            (batch
                             ((standard 'deserialize)
                              (cadr (assoc 'proof batch-response))))
                            (inspect
                             (lambda (proof)
                               (let ((running
                                      (sync-eval
                                       ((standard 'deep-get) proof
                                        '(0 (*state* alice counter))))))
                                 (list
                                  ((running 'value))
                                  (catch #t
                                    (lambda () ((running 'hidden)))
                                    (lambda args 'unavailable)))))))
                       (list
                        (equal? (sync-digest head) (sync-digest scalar))
                        (inspect scalar)
                        (equal? (sync-digest head) (sync-digest blank))
                        (let ((running
                               (sync-eval
                                ((standard 'deep-get) blank
                                 '(0 (*state* alice counter))))))
                          (list
                           (running '*name*)
                           (catch #t
                             (lambda () ((running 'value)))
                             (lambda args 'unavailable))))
                        (equal? (sync-digest head) (sync-digest batch))
                        (inspect batch))))))
     :expect '(#t (8 unavailable) #t (counter unavailable)
               #t (8 unavailable)))

    (test-submit
     ((alice journal-1 'retrieve) :path '(0 *state* alice counter)
      :method '*api* :arguments '() :expression? #t)
     :expect '(*name* *api* *class* *init* value hidden increment! mutate-error!
               bytes mutate-raw! mutate-vector! mutate-procedure!))
    (test-submit
     ((alice journal-1 'retrieve) :path '(0 *state* alice counter)
      :method 'value :arguments '() :expression? #t)
     :expect 8)
    (test-submit
     ((alice journal-1 'retrieve) :path '(0 *state* alice counter)
      :expression? #t)
     :expect (lambda (result)
               (and (equal? (cadr (assoc 'class result)) 'counter)
                    (byte-vector? (cadr (assoc 'object-hash result)))
                    (byte-vector? (cadr (assoc 'code-hash result))))))
    (test-submit
     ((alice journal-1 'retrieve-batch)
      :paths '((0 *state* alice counter)
               (0 *state* alice counter)
               (0 *state* alice counter))
      :methods '(value increment! value)
      :arguments '(() (10) ())
      :expression? #t)
     :expect (lambda (result)
               (equal? (map (lambda (entry)
                              (cadr (assoc 'content entry)))
                            (cadr (assoc 'results result)))
                       '(8 18 8))))

    ;; A full use! grant admits both mutating and read-only object access.
    (test-submit
     ((bob journal-1 'use!)
      `((path ,path) (method value) (arguments ()) (expression? #t)))
     :expect error-result?)
    (test-submit
     ((alice journal-1 'authorize!)
      '((user (*state* alice))
        (rule ((principal (*state* bob)) (path (counter))
                (put! #f) (use! ((read-only? #f))) (retrieve #f)))))
     :expect #t)
    (test-submit
     ((bob journal-1 'use!)
      `((path ,path) (method value) (arguments ()) (expression? #t)))
     :expect 8)
    (test-submit
     ((bob journal-1 'use!) path :read-only? #t)
     :expect (lambda (result)
               (and (equal? (cadr (assoc 'class result)) 'counter)
                    (byte-vector? (cadr (assoc 'object-hash result))))))

    ;; Scalar and batch expectations compare object code only, ignoring state.
    (test-submit
     ((alice journal-1 'put-batch!)
      `((paths (,path)) (values (,counter-class)) (object? (#t))
        (expected (,changed-class)) (expression? #t)))
     :expect #f)
    (test-submit
     ((alice journal-1 'put-batch!)
      `((paths (,path)) (values (,counter-class)) (object? (#t))
        (expected (,counter-class)) (expression? #t)))
     :expect #t)
    (test-submit
     ((alice journal-1 'put!)
      `((path ,path) (value ,counter-class) (object? #t)
        (expected ,changed-class) (expression? #t)))
     :expect #f)
    (test-submit
     ((alice journal-1 'put!)
      `((path ,path) (value ,counter-class) (object? #t)
        (expected ,counter-class) (expression? #t)))
     :expect #t)

    ;; Blank inert use is the read-only codec path; named inert use fails.
    (test-submit
     ((alice journal-1 'put!)
      '((path (*state* alice inert)) (value (hello world)) (expression? #t)))
     :expect #t)
    (test-submit
     ((alice journal-1 'use!)
      '((path (*state* alice inert)) (expression? #t)))
     :expect '(hello world))
    (test-submit
     ((alice journal-1 'use!)
      '((path (*state* alice inert)) (method value)
        (arguments ()) (expression? #t)))
     :expect error-result?)
    (test-submit
     ((alice journal-1 'use!)
      '((path (*state* alice inert)) (method *api*)
        (arguments ()) (read-only? #t) (expression? #t)))
     :expect error-result?)

    (test-submit
     ((alice journal-1 'put!)
      '((path (*state* alice invalid)) (value (nothing))
        (object? #t) (expression? #t)))
     :expect error-result?)
    (test-submit
     ((alice journal-1 'put!)
      '((path (*state* alice malformed)) (value (not-a-class))
        (object? #t) (expression? #t)))
     :expect error-result?)

    ;; Terminal proof generation and origin verification recalculate the exact
    ;; active method for routed scalar and same-head batch retrieve.
    (define remote-path '(*state* alice remote-counter))
    (test-submit
     ((alice journal-2 'put!)
      `((path ,remote-path) (value ,counter-class)
        (object? #t) (expression? #t)))
     :expect #t)
    (test-submit
     ((alice journal-2 'use!)
      `((path ,remote-path) (method *init*) (arguments (12))
        (expression? #t)))
     :expect 12)
    (test-submit ((*journal* journal-2 'step!)) :expect 1)
    (test-submit
     ((alice journal-2 'authorize!)
      '((user (*state* alice))
        (rule ((principal (journal-1 *state* alice))
               (key-index (-32 -1)) (path (remote-counter))
                (put! #f) (use! #f) (retrieve (0 -1))))))
     :expect #t)
    (test-submit ((*journal* journal-1 'bridge!) journal-2)
                 :schedule '(2 1) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 2)
    (test-submit ((*journal* journal-2 'step!)) :expect 2)
    (test-report)
    (test-submit ((*journal* journal-1 'bridge!) journal-2)
                 :schedule '(1 2) :expect #t)
    (test-report)
    (test-submit ((*journal* journal-1 'step!)) :expect 2)
    (test-report)
    (test-submit
     ((alice journal-1 journal-2 'retrieve)
      '(0 *state* alice remote-counter) :history '(1 1)
      :method 'value :arguments '() :expression? #t)
     :schedule '(2 1 0 1) :expect 12)
    (test-submit
     ((alice journal-1 'retrieve-batch)
      '((1 journal-2 1 *state* alice remote-counter)
        (1 journal-2 1 *state* alice remote-counter))
      :methods '(increment! value) :arguments '((10) ()) :expression? #t)
     :schedule '(2 1 0 1)
     :expect
     '((results
        (((path (1 journal-2 1 *state* alice remote-counter))
          (content 22))
         ((path (1 journal-2 1 *state* alice remote-counter))
          (content 12))))))

    (test-report)))
