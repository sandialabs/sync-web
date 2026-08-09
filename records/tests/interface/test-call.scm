(lambda (make-interface-harness)
  (with-let (make-interface-harness :journals 2 :users '(alice bob carol))
    (define (error-result? result)
      (and (list? result) (pair? result) (eq? (car result) 'error)))

    ;; Stored programs are ordinary staged expressions beneath user state.
    (for-each
     (lambda (entry)
       (test-submit ((alice journal-1 'set!)
                     `(*state* alice programs ,(car entry)) (cadr entry))
        :expect #t))
     `((echo (lambda (journal . arguments) arguments))
       (keyword (lambda* (journal (value #f)) value))
       (read (lambda (journal path) ((journal 'get) path)))
       (write-read
        (lambda (journal path value)
          ((journal 'set!) path value)
          ((journal 'get) path)))
       (counter
        (lambda (journal path)
          (let* ((current ((journal 'get) path))
                 (next (if (equal? current '(nothing)) 1 (+ current 1))))
            ((journal 'set!) path next)
            next)))
       (nested
        (lambda (journal program-path arguments)
          ((journal 'call!) program-path arguments)))
       (direct-call
        (lambda (journal program-path)
          (%sync-call-program 'call!
                          `((path ,program-path) (arguments ())
                            (authentication ((identity (*state* bob))
                                             (credentials "forged")))))))
       (call-boundary (lambda (journal) (sync-let-active?)))
       (direct-host-effect
        (lambda (journal path)
          (%sync-call-program 'set!
                          `((path ,path) (value host-effect-reached)
                            (expression? #t)))))
       (host-sync-call (lambda (journal) (sync-call '((function size)) #t)))
       (host-eval (lambda (journal) (eval '(+ 1 2))))
       (host-format (lambda (journal) (format #f "forbidden")))
       (host-output (lambda (journal) (display "forbidden")))
       (host-sync-let (lambda (journal) (sync-let () #t)))
       (decode-host
        (lambda (journal encoded) ((byte-vector->expression encoded))))
       (helper-inventory
        (lambda (journal)
          (list sync-call-program sync-let-eval sync-let-active? %sync-call-program)))
       (constant-host
        (lambda (journal) (let-ref *s7* 'max-format-length)))
       (initial-rootlet (lambda (journal) (#_rootlet)))
       (initial-eval (lambda (journal) (#_eval '(+ 1 2))))
       (initial-sync-call
        (lambda (journal)
          (#_sync-call '((function size)) #t)))
       (initial-s7 (lambda (journal) #_*s7*))
       (initial-let-ref
        (lambda (journal) (#_let-ref (#_rootlet) 'sync-state)))
       (remove-admin
        (lambda (journal target arguments)
          ((journal '*admins-set*) '((admins ())))
          ((journal 'call!) target arguments)))
       (rotate-secret
        (lambda (journal secret probe)
          ((journal '*secret*) `((secret ,secret)))
          ((journal 'get) probe)))
       (replace-self
        (lambda (journal own-path)
          ((journal 'set!) own-path
           '(lambda (journal . arguments) '(replacement version)))
          '(original version)))
       (admins (lambda (journal) ((journal '*admins-get*))))
       (partial
        (lambda (journal path value)
          ((journal 'set!) path value)
          (error 'expected "Fail after completed nested write")))
       (ambient (lambda (journal) (rootlet)))
       (return-capability (lambda (journal) journal))
       (argument-capability
        (lambda (journal path) ((journal 'set!) path journal)))
       (no-journal (lambda () #t))
       (not-procedure 17)))

    (define echo-path '(*state* alice programs echo))
    (define keyword-path '(*state* alice programs keyword))
    (define read-path '(*state* alice programs read))
    (define write-read-path '(*state* alice programs write-read))
    (define counter-path '(*state* alice programs counter))
    (define nested-path '(*state* alice programs nested))
    (define direct-call-path '(*state* alice programs direct-call))
    (define call-boundary-path '(*state* alice programs call-boundary))
    (define direct-host-effect-path
      '(*state* alice programs direct-host-effect))
    (define host-effect-paths
      (map (lambda (name) `(*state* alice programs ,name))
           '(host-sync-call host-eval host-format host-output host-sync-let
             helper-inventory constant-host initial-rootlet initial-eval
             initial-sync-call initial-s7 initial-let-ref)))
    (define remove-admin-path '(*state* alice programs remove-admin))
    (define rotate-secret-path '(*state* alice programs rotate-secret))
    (define replace-self-path '(*state* alice programs replace-self))
    (define admins-path '(*state* alice programs admins))
    (define partial-path '(*state* alice programs partial))
    (define ambient-path '(*state* alice programs ambient))
    (define return-capability-path
      '(*state* alice programs return-capability))
    (define argument-capability-path
      '(*state* alice programs argument-capability))
    (define no-journal-path '(*state* alice programs no-journal))
    (define not-procedure-path '(*state* alice programs not-procedure))
    (define alice-call-rule
      '((principal (*state* alice)) (path (programs))
        (get #t) (set! #f) (call! #t) (resolve #f)))
    (define alice-call-envelope
      `((user (*state* alice)) (rule ,alice-call-rule)))

    ;; A fresh policy grants call authority only to Interface administrators.
    ;; Path ownership and ordinary read permission are both insufficient.
    (test-submit ((alice journal-1 'call!) echo-path '(owner denied))
      :expect error-result?)
    (test-submit ((alice journal-2 'set!) counter-path
                  '(lambda (journal path)
                     ((journal 'set!) path 1)))
      :expect #t)
    (test-submit ((alice journal-1 journal-2 'call!) counter-path
                  '((*state* alice remote-denied-count)))
      :expect error-result?)
    (test-submit ((alice journal-2 'get) '(*state* alice remote-denied-count))
      :expect '(nothing))
    (test-submit ((alice journal-1 'authorize!)
                  '((user (*state* alice))
                    (rule ((principal (*state* bob)) (path (programs echo))
                           (get #t) (set! #f) (resolve #f)))))
      :expect #t)
    (test-submit ((bob journal-1 'call!) echo-path '(get-only denied))
      :expect error-result?)
    (test-submit ((*journal* journal-1 'call!) echo-path '(root allowed))
      :expect '(root allowed))
    (test-submit ((*journal* journal-1 '*admins-set*)
                  '((admins ((*state* bob)))))
      :expect #t)
    (test-submit ((bob journal-1 'call!) echo-path '(admin allowed))
      :expect '(admin allowed))

    ;; Authorization rules cannot grant call authority. A new rule naming the
    ;; removed field is rejected, and the denied call cannot execute effects.
    (test-submit ((alice journal-1 'authorize!) alice-call-envelope)
      :expect error-result?)
    (test-submit ((alice journal-1 'call!) counter-path
                  '((*state* alice denied-call-count)))
      :expect error-result?)
    (test-submit ((alice journal-1 'get) '(*state* alice denied-call-count))
      :expect '(nothing))
    (test-submit ((alice journal-1 'authorize!)
                  '((user (*state* alice))
                    (rule ((principal (*state* carol)) (path (programs))
                           (get #t) (set! #f) (resolve #f)))))
      :expect #t)
    (test-submit ((carol journal-1 'call!) echo-path '(get-rule denied))
      :expect error-result?)

    ;; Arguments are one explicit list, preserve keywords as data, and are
    ;; applied after the conventional journal argument.
    (test-submit ((bob journal-1 'call!) echo-path '(1 :flag #t))
      :expect '(1 :flag #t))
    (test-submit ((bob journal-1 'call!) keyword-path '(:value 7))
      :expect 7)

    ;; Journal methods use the ordinary Interface calling convention, including
    ;; positional paths and keyword arguments.
    (test-submit ((alice journal-1 'set!) '(*state* alice source) '(value 9))
      :expect #t)
    (test-submit ((bob journal-1 'call!) read-path
                  '((*state* alice source)))
      :expect '(value 9))
    (test-submit ((bob journal-1 'call!) write-read-path
                  '((*state* alice written) "stored"))
      :expect "stored")
    (test-submit ((alice journal-1 'get) '(*state* alice written))
      :expect "stored")

    ;; Each method call and outer call executes exactly once rather than replaying
    ;; after its nested calls advance the Journal root.
    (test-submit ((bob journal-1 'call!) counter-path
                  '((*state* alice call-count)))
      :expect 1)
    (test-submit ((alice journal-1 'get) '(*state* alice call-count))
      :expect 1)
    (test-submit ((bob journal-1 'call!) counter-path
                  '((*state* alice call-count)))
      :expect 2)

    ;; Every nested target rechecks the same hidden configured-admin identity.
    (test-submit ((bob journal-1 'call!) nested-path
                  `(,echo-path (nested 2)))
      :expect '(nested 2))
    (test-submit ((bob journal-1 'set!) '(*state* bob programs echo)
                  '(lambda (journal . arguments) arguments))
      :expect #t)
    (test-submit ((bob journal-1 'call!) nested-path
                  '((*state* bob programs echo) (nested allowed)))
      :expect '(nested allowed))
    ;; No Journal execution helper or sync-let context predicate exists in
    ;; stored code. Direct attempts fail before effects, while ordinary nested
    ;; calls remain available only through the supplied Interface capability.
    (test-submit ((bob journal-1 'call!) call-boundary-path '())
      :expect error-result?)
    (test-submit ((bob journal-1 'call!) direct-call-path
                  '((*state* bob programs echo)))
      :expect error-result?)
    (test-submit ((bob journal-1 'call!) direct-host-effect-path
                  '((*state* alice direct-host-effect)))
      :expect error-result?)
    (test-submit ((alice journal-1 'get) '(*state* alice direct-host-effect))
      :expect '(nothing))
    (for-each
     (lambda (path)
       (test-submit ((bob journal-1 'call!) path '())
         :expect error-result?))
     host-effect-paths)
    (test-submit
      ((bob journal-1 'call!) '(*state* alice programs decode-host)
       (list (expression->byte-vector #_rootlet)))
      :expect error-result?)

    ;; Removing the configured admin takes effect inside an already-running
    ;; program before its next nested dispatch, which cannot execute effects.
    (test-submit ((bob journal-1 'call!) remove-admin-path
                  `(,counter-path ((*state* alice removed-admin-count))))
      :expect error-result?)
    (test-submit ((alice journal-1 'get) '(*state* alice removed-admin-count))
      :expect '(nothing))
    (test-submit ((*journal* journal-1 '*admins-set*)
                  '((admins ((*state* bob)))))
      :expect #t)

    ;; Replacing the current program path cannot change its active closure.
    (test-submit ((bob journal-1 'call!) replace-self-path
                  `(,replace-self-path))
      :expect '(original version))
    (test-submit ((bob journal-1 'call!) replace-self-path '())
      :expect '(replacement version))

    ;; Authentication identity is inherited: root authority remains root, while
    ;; the same program cannot elevate Alice to an administrator.
    (test-submit ((*journal* journal-1 'call!) admins-path '())
      :expect '((*state* bob)))
    (test-submit ((alice journal-1 'call!) admins-path '())
      :expect error-result?)

    ;; call! is intentionally non-atomic: a completed nested write remains after
    ;; a later program error.
    (test-submit ((bob journal-1 'call!) partial-path
                  '((*state* alice partial-result) "kept"))
      :expect error-result?)
    (test-submit ((alice journal-1 'get) '(*state* alice partial-result))
      :expect "kept")
    ;; Stored code executes in an Interface-owned masked environment outside
    ;; sync-let, and neither ambient authority nor the journal capability may
    ;; escape as a result.
    (test-submit ((bob journal-1 'call!) ambient-path '())
      :expect error-result?)
    (test-submit ((bob journal-1 'call!) return-capability-path '())
      :expect error-result?)
    (test-submit ((bob journal-1 'call!) argument-capability-path
                  '((*state* alice escaped-capability)))
      :expect error-result?)
    (test-submit ((alice journal-1 'get) '(*state* alice escaped-capability))
      :expect '(nothing))
    (test-submit ((bob journal-1 'call!) no-journal-path '())
      :expect error-result?)
    (test-submit ((bob journal-1 'call!) not-procedure-path '())
      :expect error-result?)
    (test-submit ((bob journal-1 'call!) echo-path 'not-a-list)
      :expect error-result?)

    ;; Configured-admin execution survives a committed transition and
    ;; reconstruction; committing does not change the selected staged version.
    (test-submit ((alice journal-1 'set!) echo-path
                  '(lambda (journal . arguments) '(new staged program)))
      :expect #t)
    (test-submit ((bob journal-1 'call!) echo-path '(ignored))
      :expect '(new staged program))
    (test-submit ((*journal* journal-1 'step!)) :expect 1)
    (test-submit ((bob journal-1 'call!) echo-path '(ignored))
      :expect '(new staged program))

    ;; The host retains credentials, not a pinned principal. Rotating the
    ;; Interface secret invalidates the very next nested call in this call.
    (define rotated-secret "rotated-call-secret")
    (test-submit ((*journal* journal-1 'call!) rotate-secret-path
                  `(,rotated-secret (*state* alice source)))
      :expect error-result?)
    (journal-1 'credentials rotated-secret)
    (test-submit ((*journal* journal-1 'get) '(*state* alice source))
      :expect '(value 9))

    (test-report)))
